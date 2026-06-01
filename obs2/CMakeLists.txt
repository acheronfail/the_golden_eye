cmake_minimum_required(VERSION 3.16)

set(PLUGIN_NAME the_golden_eye)

project(${PLUGIN_NAME} C CXX)

# Force the compiler to use C11 for standard thread support
set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# OpenCV's API is C++; cv_wrapper.cpp bridges it to the C plugin
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Vendored OBS headers (populated by `just obs-headers`)
set(VENDOR_OBS_DIR "${CMAKE_CURRENT_SOURCE_DIR}/vendor/obs")
set(VENDOR_LIBOBS_DIR "${VENDOR_OBS_DIR}/libobs")
set(VENDOR_FRONTEND_DIR "${VENDOR_OBS_DIR}/frontend")

if(NOT EXISTS "${VENDOR_LIBOBS_DIR}/obs-module.h")
    message(FATAL_ERROR "Vendored OBS headers not found. Run 'just obs-headers' first.")
endif()

# Generate obsconfig.h from the vendored template
set(OBS_RELEASE_CANDIDATE 0)
set(OBS_BETA 0)
configure_file(
    "${VENDOR_LIBOBS_DIR}/obsconfig.h.in"
    "${CMAKE_CURRENT_BINARY_DIR}/obsconfig.h"
)

# OpenCV (Homebrew on macOS, libopencv-dev on Linux both ship CMake configs).
# Only request the modules we actually use to keep link size/time down.
# imgcodecs is needed to load the per-user template PNGs from disk.
find_package(OpenCV REQUIRED COMPONENTS core imgproc imgcodecs)

# Find OBS libraries to link against
if(APPLE)
    set(OBS_FW_DIR "/Applications/OBS.app/Contents/Frameworks")
    set(OBS_LIBRARY "${OBS_FW_DIR}/libobs.framework/libobs")
    set(OBS_FRONTEND_LIBRARY "${OBS_FW_DIR}/obs-frontend-api.dylib")
    foreach(_lib OBS_LIBRARY OBS_FRONTEND_LIBRARY)
        if(NOT EXISTS "${${_lib}}")
            message(FATAL_ERROR "Could not find OBS library: ${${_lib}}")
        endif()
    endforeach()
    set(OBS_LIBRARIES ${OBS_LIBRARY})
    set(OBS_FRONTEND_LIBRARIES ${OBS_FRONTEND_LIBRARY})
else()
    find_package(PkgConfig REQUIRED)
    pkg_check_modules(OBS REQUIRED libobs)

    # Arch exposes obs-frontend-api via pkg-config, but Debian-based distros
    # often do not. Fall back to finding the shared library directly.
    pkg_check_modules(OBS_FRONTEND QUIET obs-frontend-api)
    if(NOT OBS_FRONTEND_FOUND)
        find_library(OBS_FRONTEND_LIBRARY
            NAMES obs-frontend-api libobs-frontend-api
            HINTS ${OBS_LIBRARY_DIRS}
            PATH_SUFFIXES lib lib64 x86_64-linux-gnu
        )
        if(NOT OBS_FRONTEND_LIBRARY)
            message(FATAL_ERROR
                "Could not find OBS frontend API library. Install OBS development files for your distro."
            )
        endif()
        set(OBS_FRONTEND_LIBRARIES ${OBS_FRONTEND_LIBRARY})
    endif()
endif()

if(APPLE)
    add_library(${PLUGIN_NAME} MODULE)
else()
    add_library(${PLUGIN_NAME} SHARED)
endif()

target_sources(${PLUGIN_NAME} PRIVATE
    bmp.c
    cv_wrapper.cpp
    http.c
    logger.c
    obs_bridge.c
    plugin.c
    stream_notifier.c
    vendor/mongoose.c
)

target_include_directories(${PLUGIN_NAME} PRIVATE
    # Root `vendor/` dir
    ${CMAKE_CURRENT_SOURCE_DIR}/vendor
    # Also expose `libobs/` directly so OBS's headers can #include <obs.h> etc.
    ${VENDOR_LIBOBS_DIR}
    ${CMAKE_CURRENT_BINARY_DIR}
    $<$<BOOL:${APPLE}>:/opt/homebrew/opt/simde/include>
    $<$<BOOL:${APPLE}>:${CMAKE_CURRENT_SOURCE_DIR}/compat>
    ${OpenCV_INCLUDE_DIRS}
)
target_link_libraries(${PLUGIN_NAME} PRIVATE
    ${OBS_LIBRARIES}
    ${OBS_FRONTEND_LIBRARIES}
    ${OpenCV_LIBS}
)

if(WIN32)
    target_link_libraries(${PLUGIN_NAME} PRIVATE ws2_32)
endif()

#
# Standalone CLI for exercising ge_cv_match_level() against a PNG on disk.
#

add_executable(test_match test_match.cpp cv_wrapper.cpp)
target_include_directories(test_match PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${OpenCV_INCLUDE_DIRS}
)
target_compile_definitions(test_match PRIVATE
    GE_TEMPLATES_DIR="${CMAKE_CURRENT_SOURCE_DIR}/templates"
)
target_link_libraries(test_match PRIVATE ${OpenCV_LIBS})

#
# Plugin output:
#   a plugin bundle on macOS, and a regular shared library on other platforms.
#

if(APPLE)
    set_target_properties(${PLUGIN_NAME} PROPERTIES
        BUNDLE TRUE
        BUNDLE_EXTENSION "plugin"
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/Info.plist.in"
        PREFIX ""
    )
else()
    set_target_properties(${PLUGIN_NAME} PROPERTIES PREFIX "")
endif()
