cmake_minimum_required(VERSION 3.16)

set(PLUGIN_NAME the_golden_eye)

project(${PLUGIN_NAME} C)

# Force the compiler to use C11 for standard thread support
set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

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
    pkg_check_modules(OBS_FRONTEND REQUIRED obs-frontend-api)
endif()

if(APPLE)
    add_library(${PLUGIN_NAME} MODULE)
else()
    add_library(${PLUGIN_NAME} SHARED)
endif()

target_sources(${PLUGIN_NAME} PRIVATE
    bmp.c
    http.c
    logger.c
    obs_bridge.c
    plugin.c
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
)
target_link_libraries(${PLUGIN_NAME} PRIVATE
    ${OBS_LIBRARIES}
    ${OBS_FRONTEND_LIBRARIES}
)

if(WIN32)
    target_link_libraries(${PLUGIN_NAME} PRIVATE ws2_32)
endif()

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