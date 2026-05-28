cmake_minimum_required(VERSION 3.16)

set(PLUGIN_NAME the_golden_eye)

project(${PLUGIN_NAME} C)

# Force the compiler to use C11 for standard thread support
set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(PkgConfig REQUIRED)
pkg_check_modules(OBS REQUIRED libobs)
pkg_check_modules(OBS_FRONTEND REQUIRED obs-frontend-api)

# Some distros install OBS headers under /usr/include/obs while pkg-config
# only reports /usr/include. Resolve obs.h explicitly to keep includes portable.
find_path(OBS_HEADERS_DIR
    NAMES obs.h
    PATH_SUFFIXES obs
)

if(NOT OBS_HEADERS_DIR)
    message(FATAL_ERROR "Could not locate obs.h. Install OBS development headers.")
endif()

add_library(${PLUGIN_NAME} SHARED)

target_sources(${PLUGIN_NAME} PRIVATE
    bmp.c
    http.c
    logger.c
    obs_bridge.c
    plugin.c
    vendor/mongoose.c
)

target_include_directories(${PLUGIN_NAME} PRIVATE
    ${OBS_INCLUDE_DIRS}
    ${OBS_FRONTEND_INCLUDE_DIRS}
    ${OBS_HEADERS_DIR}
)
target_link_libraries(${PLUGIN_NAME} PRIVATE
    ${OBS_LIBRARIES}
    ${OBS_FRONTEND_LIBRARIES}
)

if(WIN32)
    target_link_libraries(${PLUGIN_NAME} PRIVATE ws2_32)
endif()

set_target_properties(${PLUGIN_NAME} PROPERTIES PREFIX "")