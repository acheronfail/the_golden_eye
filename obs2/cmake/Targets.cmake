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
set(CORE_NAME golden_core)

# Runtime bundle layout:
# - macOS: OBS loads the .plugin bundle executable from Contents/MacOS, so keep
#   the core next to it and put templates in the bundle resources directory.
# - Linux: OBS loads the plugin shared object from the build/install plugin
#   directory, so keep the core and templates beside it.
if(APPLE)
  set(GE_PLUGIN_RUNTIME_DIR "${CMAKE_CURRENT_BINARY_DIR}/${PLUGIN_NAME}.plugin/Contents/MacOS")
  set(GE_BUNDLED_TEMPLATE_DIR "${CMAKE_CURRENT_BINARY_DIR}/${PLUGIN_NAME}.plugin/Contents/Resources/cv_templates")
  set(GE_BUNDLED_TEMPLATE_DIR_REL "../Resources/cv_templates")
else()
  set(GE_PLUGIN_RUNTIME_DIR "${CMAKE_CURRENT_BINARY_DIR}")
  set(GE_BUNDLED_TEMPLATE_DIR "${CMAKE_CURRENT_BINARY_DIR}/cv_templates")
  set(GE_BUNDLED_TEMPLATE_DIR_REL "cv_templates")
endif()

file(GLOB GE_CV_TEMPLATE_FILES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/cv_templates/*.png"
)

set(GE_BUNDLED_TEMPLATE_STAMP "${GE_BUNDLED_TEMPLATE_DIR}/.stamp")
add_custom_command(
    OUTPUT "${GE_BUNDLED_TEMPLATE_STAMP}"
    COMMAND ${CMAKE_COMMAND} -E rm -rf "${GE_BUNDLED_TEMPLATE_DIR}"
    COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_BUNDLED_TEMPLATE_DIR}"
    COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${CMAKE_CURRENT_SOURCE_DIR}/cv_templates"
            "${GE_BUNDLED_TEMPLATE_DIR}"
    COMMAND ${CMAKE_COMMAND} -E touch "${GE_BUNDLED_TEMPLATE_STAMP}"
    DEPENDS ${GE_CV_TEMPLATE_FILES}
    COMMENT "Bundling CV templates"
    VERBATIM
)

add_custom_target(bundle_cv_templates ALL
    DEPENDS "${GE_BUNDLED_TEMPLATE_STAMP}"
)

add_library(${CORE_NAME} SHARED)

target_sources(${CORE_NAME} PRIVATE
    obs_bridge.c
    core.c
)

if(NOT GE_OBS_NATIVE_DEPS_FOUND)
  add_custom_target(require_obs_libraries
      COMMAND ${CMAKE_COMMAND} -E echo
              "Native OBS build dependencies are required to build plugin targets."
      COMMAND ${CMAKE_COMMAND} -E echo
              "On Linux, use 'just obs', 'just make-package', or build inside the OBS Flatpak SDK."
      COMMAND ${CMAKE_COMMAND} -E false
      VERBATIM
  )
  add_dependencies(${CORE_NAME} require_obs_libraries)
endif()

# Emit the core library into the same runtime directory the shim will resolve
# from when OBS loads it.
set_target_properties(${CORE_NAME} PROPERTIES
    LIBRARY_OUTPUT_DIRECTORY "${GE_PLUGIN_RUNTIME_DIR}"
    RUNTIME_OUTPUT_DIRECTORY "${GE_PLUGIN_RUNTIME_DIR}"
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
    ${GE_SIMDE_INCLUDE_DIR}
)
target_link_libraries(${CORE_NAME} PRIVATE
    ${OBS_LIBRARIES}
    ${OBS_FRONTEND_LIBRARIES}
    rust_libs
    # Pulls in the OpenCV static archives the C++ shim in libge_rust.a references
    # (cv::Mat, cv::imgproc, ...). Listed after rust_libs so the archives that
    # define these symbols come after the archive that uses them on the link line.
    ${GE_OPENCV_LINK}
    # Likewise, the FFmpeg static archives that ge_rust.a references via the
    # ffmpeg-next crate (libav*, libsw*). Also listed after rust_libs.
    ${GE_FFMPEG_LINK}
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
  # Drop NEEDED entries for shared libraries whose symbols are never actually
  # referenced.  Static-archive dependency lists from OpenCV/FFmpeg pkg-config
  # can pull in transitive shared libs (libva, libva-drm, …) that aren't
  # present in constrained environments like the OBS Flatpak sandbox.
  target_link_options(${CORE_NAME} PRIVATE "LINKER:--as-needed")
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

if(NOT GE_OBS_NATIVE_DEPS_FOUND)
  add_dependencies(${PLUGIN_NAME} require_obs_libraries)
endif()

# The hot-reload watcher is dev-only; it isn't compiled into release builds.
if(BROWSER_DEV)
  target_sources(${PLUGIN_NAME} PRIVATE dev_reload.c)
endif()

target_include_directories(${PLUGIN_NAME} PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/vendor
    ${VENDOR_LIBOBS_DIR}
    ${CMAKE_CURRENT_BINARY_DIR}
    # obs-module.h transitively pulls in obs.h, which needs simde.
    ${GE_SIMDE_INCLUDE_DIR}
)

find_package(Threads REQUIRED)

target_link_libraries(${PLUGIN_NAME} PRIVATE
    ${OBS_LIBRARIES}
    ${CMAKE_DL_LIBS}
    Threads::Threads
)

# Bake in only relative bundle names. The shim resolves them from the loaded
# plugin path at runtime, so the built plugin can be copied out of this repo.
target_compile_definitions(${PLUGIN_NAME} PRIVATE
    GE_CORE_LIB_NAME="$<TARGET_FILE_NAME:${CORE_NAME}>"
    GE_BUNDLED_TEMPLATE_DIR_REL="${GE_BUNDLED_TEMPLATE_DIR_REL}"
    # In dev mode the shim copies + hot-reloads the core library on rebuild.
    $<$<BOOL:${BROWSER_DEV}>:GE_DEV>
)

# Build the core library and bundled templates whenever the plugin is built.
add_dependencies(${PLUGIN_NAME} ${CORE_NAME})
add_dependencies(${CORE_NAME} bundle_cv_templates)
add_dependencies(${PLUGIN_NAME} bundle_cv_templates)

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
