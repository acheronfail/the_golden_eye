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
# - Linux: OBS's module scanner expects a plugin-shaped directory with
#   bin/<arch> for libraries and data/ for module files.
if(APPLE)
  set(GE_PLUGIN_RUNTIME_DIR "${CMAKE_CURRENT_BINARY_DIR}/${PLUGIN_NAME}.plugin/Contents/MacOS")
  set(GE_PLUGIN_DATA_DIR "${CMAKE_CURRENT_BINARY_DIR}/${PLUGIN_NAME}.plugin/Contents/Resources")
elseif(UNIX)
  set(GE_PLUGIN_RUNTIME_DIR "${CMAKE_CURRENT_BINARY_DIR}/${PLUGIN_NAME}/bin/${GE_OBS_ARCH_DIR}")
  set(GE_PLUGIN_DATA_DIR "${CMAKE_CURRENT_BINARY_DIR}/${PLUGIN_NAME}/data")
else()
  set(GE_PLUGIN_RUNTIME_DIR "${CMAKE_CURRENT_BINARY_DIR}")
  set(GE_PLUGIN_DATA_DIR "${CMAKE_CURRENT_BINARY_DIR}")
endif()
set(GE_BUNDLED_TEMPLATE_DIR "${GE_PLUGIN_DATA_DIR}/cv_templates")

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
    core/obs_bridge.c
    core/core.c
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

if(APPLE AND GE_OBS_DYNAMIC_LOOKUP)
  target_link_options(${CORE_NAME} PRIVATE "LINKER:-undefined,dynamic_lookup")
endif()

if(APPLE)
  if(GE_RUST_PACKAGE_PROFILE)
    target_link_options(${CORE_NAME} PRIVATE
        "LINKER:-exported_symbols_list,${CMAKE_CURRENT_SOURCE_DIR}/cmake/golden_core.macos.exports"
    )
  endif()
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
elseif(WIN32)
  target_link_libraries(${CORE_NAME} PRIVATE
      bcrypt
      ntdll
      ole32
      secur32
      user32
      ws2_32
  )
else()
  set(GE_LINUX_CORE_LINK_OPTIONS "LINKER:--as-needed")
  if(GE_RUST_PACKAGE_PROFILE)
    list(APPEND GE_LINUX_CORE_LINK_OPTIONS
        "LINKER:--version-script=${CMAKE_CURRENT_SOURCE_DIR}/cmake/golden_core.linux.version"
    )
  endif()
  # On Linux, the C++ shim from opencv-rust needs libstdc++.
  target_link_libraries(${CORE_NAME} PRIVATE stdc++)
  # Drop NEEDED entries for shared libraries whose symbols are never actually
  # referenced.  Static-archive dependency lists from OpenCV/FFmpeg pkg-config
  # can pull in transitive shared libs (libva, libva-drm, …) that aren't
  # present in constrained environments like the OBS Flatpak sandbox.
  target_link_options(${CORE_NAME} PRIVATE ${GE_LINUX_CORE_LINK_OPTIONS})
endif()

#
# Thin shim — the actual OBS plugin.
#
# A plugin bundle on macOS, and a regular shared library on other platforms.
# It only dlopen's the core library, so it links nothing heavy: just libobs
# (for the OBS module macros + blog), the dl loader, and pthreads (the reload
# worker thread in shim/reload.c).
#

if(APPLE)
  add_library(${PLUGIN_NAME} MODULE)
else()
  add_library(${PLUGIN_NAME} SHARED)
endif()

target_sources(${PLUGIN_NAME} PRIVATE
    shim/dynlib.c
    shim/reload.c
    shim/plugin.c
)

if(NOT GE_OBS_NATIVE_DEPS_FOUND)
  add_dependencies(${PLUGIN_NAME} require_obs_libraries)
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

if(APPLE AND GE_OBS_DYNAMIC_LOOKUP)
  target_link_options(${PLUGIN_NAME} PRIVATE "LINKER:-undefined,dynamic_lookup")
endif()

# Bake in only relative bundle names. The shim resolves them from the loaded
# plugin path at runtime, so the built plugin can be copied out of this repo.
target_compile_definitions(${PLUGIN_NAME} PRIVATE
    GE_CORE_LIB_NAME="$<TARGET_FILE_NAME:${CORE_NAME}>"
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
elseif(UNIX)
  set_target_properties(${PLUGIN_NAME} PROPERTIES
        PREFIX ""
        LIBRARY_OUTPUT_DIRECTORY "${GE_PLUGIN_RUNTIME_DIR}"
        RUNTIME_OUTPUT_DIRECTORY "${GE_PLUGIN_RUNTIME_DIR}"
    )
else()
  set_target_properties(${PLUGIN_NAME} PROPERTIES PREFIX "")
endif()
