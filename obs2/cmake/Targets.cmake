# The two build artifacts: the heavy "core" library and the thin shim OBS loads.
# Must be included last — depends on OBSLibs (OBS_*), OpenCVStatic (GE_OPENCV_LINK),
# RustLib (rust_libs), and Frontend (BROWSER_DEV).

#
# Core library
#
# All of the heavy logic — the Rust staticlib, OpenCV, and the OBS bridge —
# lives in a separate shared library. The thin shim (below) is what OBS loads
# as a plugin; it `dlopen`s this core library at runtime. Keeping them apart
# lets the shim unload + reload this library when it's rebuilt (dev hot reload)
# and surface link errors as a catchable dlopen failure.
#
# The core library name (used by the shim to find it).
set(CORE_NAME ${PLUGIN_NAME}_core)

add_library(${CORE_NAME} SHARED)

target_sources(${CORE_NAME} PRIVATE
    obs_bridge.c
    core.c
)

# Emit the core library into a subdirectory so OBS's plugin scan (which loads
# top-level shared libraries / .plugin bundles from OBS_PLUGINS_PATH) doesn't
# try to load it as a plugin in its own right.
set_target_properties(${CORE_NAME} PROPERTIES
    LIBRARY_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/core"
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/core"
)

if(APPLE)
  # The core links libobs / obs-frontend-api by their @rpath install names. When
  # OBS loaded the old monolithic plugin directly, dyld resolved those via OBS's
  # own executable rpaths. The core is now dlopen'd one level removed (by the
  # shim), so give it an explicit rpath to OBS's Frameworks dir to guarantee
  # resolution regardless of dyld's load-chain rpath behaviour.
  set_target_properties(${CORE_NAME} PROPERTIES
        BUILD_RPATH "${OBS_FW_DIR}"
        INSTALL_RPATH "${OBS_FW_DIR}"
    )
endif()

target_include_directories(${CORE_NAME} PRIVATE
    # Root `vendor/` dir
    ${CMAKE_CURRENT_SOURCE_DIR}/vendor
    # Also expose `libobs/` directly so OBS's headers can #include <obs.h> etc.
    ${VENDOR_LIBOBS_DIR}
    ${CMAKE_CURRENT_BINARY_DIR}
    $<$<BOOL:${APPLE}>:/opt/homebrew/opt/simde/include>
)
target_link_libraries(${CORE_NAME} PRIVATE
    ${OBS_LIBRARIES}
    ${OBS_FRONTEND_LIBRARIES}
    rust_libs
    # Pulls in the OpenCV static archives the C++ shim in libge_rust.a references
    # (cv::Mat, cv::imgproc, ...). Listed after rust_libs so the archives that
    # define these symbols come after the archive that uses them on the link line.
    ${GE_OPENCV_LINK}
)

if(APPLE)
  # Rust's stdlib (embedded in the staticlib) references these system
  # frameworks on macOS. find_library resolves them to full paths so the
  # linker can locate them regardless of SDK layout.
  find_library(SECURITY_FW             Security             REQUIRED)
  find_library(FOUNDATION_FW           Foundation           REQUIRED)
  find_library(SYSTEM_CONFIGURATION_FW SystemConfiguration  REQUIRED)
  find_library(CORE_FOUNDATION_FW      CoreFoundation       REQUIRED)
  find_library(APPKIT_FW               AppKit               REQUIRED)
  target_link_libraries(${CORE_NAME} PRIVATE
        ${SECURITY_FW}
        ${FOUNDATION_FW}
        ${SYSTEM_CONFIGURATION_FW}
        ${CORE_FOUNDATION_FW}
        ${APPKIT_FW}
        -lresolv
        # libc++ provides the C++ runtime symbols the opencv-rust shim
        # depends on (operator new/delete, __cxa_*, __gxx_personality_v0).
        c++
        # The opencv-rust shim also links iconv on macOS.
        iconv
    )
else()
  # On Linux, the C++ shim from opencv-rust needs libstdc++.
  target_link_libraries(${CORE_NAME} PRIVATE stdc++)
endif()

if(WIN32)
  target_link_libraries(${CORE_NAME} PRIVATE ws2_32)
endif()

#
# Thin shim — the actual OBS plugin.
#
# A plugin bundle on macOS, and a regular shared library on other platforms.
# It only dlopen's the core library, so it links nothing heavy: just libobs
# (for the OBS module macros + blog), the dl loader, and pthreads (dev watcher).
#

if(APPLE)
  add_library(${PLUGIN_NAME} MODULE)
else()
  add_library(${PLUGIN_NAME} SHARED)
endif()

target_sources(${PLUGIN_NAME} PRIVATE
    plugin.c
)

# The hot-reload watcher is dev-only; it isn't compiled into release builds.
if(BROWSER_DEV)
  target_sources(${PLUGIN_NAME} PRIVATE dev_reload.c)
endif()

target_include_directories(${PLUGIN_NAME} PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/vendor
    ${VENDOR_LIBOBS_DIR}
    ${CMAKE_CURRENT_BINARY_DIR}
    # obs-module.h transitively pulls in obs.h, which needs simde on macOS.
    $<$<BOOL:${APPLE}>:/opt/homebrew/opt/simde/include>
)

find_package(Threads REQUIRED)

target_link_libraries(${PLUGIN_NAME} PRIVATE
    ${OBS_LIBRARIES}
    ${CMAKE_DL_LIBS}
    Threads::Threads
)

# Bake in the absolute path of the core library so the shim can find it; can be
# overridden at runtime with the GE_CORE_LIB env var.
target_compile_definitions(${PLUGIN_NAME} PRIVATE
    GE_CORE_LIB_PATH="$<TARGET_FILE:${CORE_NAME}>"
    # In dev mode the shim copies + hot-reloads the core library on rebuild.
    $<$<BOOL:${BROWSER_DEV}>:GE_DEV>
)

# Build the core library whenever the plugin is built.
add_dependencies(${PLUGIN_NAME} ${CORE_NAME})

if(APPLE)
  set_target_properties(${PLUGIN_NAME} PROPERTIES
        BUNDLE TRUE
        BUNDLE_EXTENSION "plugin"
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/templates/Info.plist.in"
        PREFIX ""
    )
else()
  set_target_properties(${PLUGIN_NAME} PROPERTIES PREFIX "")
endif()
