# Standalone project: exercises shim/reload.c's dlopen/rename/rollback
# mechanics directly against small fixture libraries. Deliberately has no
# dependency on OBS, OpenCV, FFmpeg, or the Rust toolchain, so it configures
# and builds fast and can run as an early, cheap CI step -- see `just
# test-shim`.

cmake_minimum_required(VERSION 3.16)
project(ge_shim_tests C)

enable_testing()
find_package(Threads REQUIRED)

set(GE_FIXTURE_DIR "${CMAKE_CURRENT_BINARY_DIR}/fixtures")

add_library(ge_fixture_v1 SHARED fixture.c)
target_compile_definitions(ge_fixture_v1 PRIVATE GE_FIXTURE_GENERATION=1)
set_target_properties(ge_fixture_v1 PROPERTIES
    PREFIX ""
    LIBRARY_OUTPUT_DIRECTORY "${GE_FIXTURE_DIR}"
    RUNTIME_OUTPUT_DIRECTORY "${GE_FIXTURE_DIR}"
)

add_library(ge_fixture_v2 SHARED fixture.c)
target_compile_definitions(ge_fixture_v2 PRIVATE GE_FIXTURE_GENERATION=2)
set_target_properties(ge_fixture_v2 PROPERTIES
    PREFIX ""
    LIBRARY_OUTPUT_DIRECTORY "${GE_FIXTURE_DIR}"
    RUNTIME_OUTPUT_DIRECTORY "${GE_FIXTURE_DIR}"
)

add_library(ge_fixture_bad SHARED fixture.c)
target_compile_definitions(ge_fixture_bad PRIVATE GE_FIXTURE_GENERATION=99 GE_FIXTURE_LOAD_FAILS=1)
set_target_properties(ge_fixture_bad PROPERTIES
    PREFIX ""
    LIBRARY_OUTPUT_DIRECTORY "${GE_FIXTURE_DIR}"
    RUNTIME_OUTPUT_DIRECTORY "${GE_FIXTURE_DIR}"
)

if(WIN32)
    set(GE_RELOAD_PLATFORM_SRC ../reload_win32.c)
else()
    set(GE_RELOAD_PLATFORM_SRC ../reload_unix.c)
endif()

add_executable(ge_shim_reload_tests
    ../dynlib.c
    ../reload.c
    ${GE_RELOAD_PLATFORM_SRC}
    test_reload.c
)
target_include_directories(ge_shim_reload_tests PRIVATE .. .)
target_link_libraries(ge_shim_reload_tests PRIVATE ${CMAKE_DL_LIBS} Threads::Threads)
add_dependencies(ge_shim_reload_tests ge_fixture_v1 ge_fixture_v2 ge_fixture_bad)

add_test(NAME shim_reload
    COMMAND ge_shim_reload_tests
            "$<TARGET_FILE:ge_fixture_v1>"
            "$<TARGET_FILE:ge_fixture_v2>"
            "$<TARGET_FILE:ge_fixture_bad>"
)

# dynlib.c's own dlopen/dlsym/dlclose/self-path contract, including failure
# paths reload.c never happens to trigger.
add_executable(ge_shim_test_dynlib
    ../dynlib.c
    test_dynlib.c
)
target_include_directories(ge_shim_test_dynlib PRIVATE .. .)
target_link_libraries(ge_shim_test_dynlib PRIVATE ${CMAKE_DL_LIBS})
add_dependencies(ge_shim_test_dynlib ge_fixture_v1)

add_test(NAME shim_dynlib
    COMMAND ge_shim_test_dynlib "$<TARGET_FILE:ge_fixture_v1>"
)

# The platform primitives declared in reload_platform.h (temp paths, atomic
# replace, recursive delete, the worker's mutex+condvar pair, thread
# spawn/join), each checked directly rather than only through a full reload.
add_executable(ge_shim_test_platform
    ${GE_RELOAD_PLATFORM_SRC}
    test_platform.c
)
target_include_directories(ge_shim_test_platform PRIVATE .. .)
target_link_libraries(ge_shim_test_platform PRIVATE Threads::Threads)

add_test(NAME shim_platform COMMAND ge_shim_test_platform)

# plugin.c itself, unmodified, linked against a small fake OBS module
# registry (fake_obs.c) instead of the real OBS SDK -- see fake_obs.h.
add_executable(ge_shim_test_plugin
    ../plugin.c
    ../dynlib.c
    ../reload.c
    ../version.c
    ${GE_RELOAD_PLATFORM_SRC}
    fake_obs.c
    test_plugin.c
)
target_include_directories(ge_shim_test_plugin PRIVATE .. . fake_obs)
target_compile_definitions(ge_shim_test_plugin PRIVATE
    GE_CORE_LIB_NAME="unused_bundled_core_name"
    GE_SHIM_VERSION="0.0.0-test"
)
target_link_libraries(ge_shim_test_plugin PRIVATE ${CMAKE_DL_LIBS} Threads::Threads)
add_dependencies(ge_shim_test_plugin ge_fixture_v1 ge_fixture_bad)

add_test(NAME shim_plugin
    COMMAND ge_shim_test_plugin
            "$<TARGET_FILE:ge_fixture_v1>"
            "$<TARGET_FILE:ge_fixture_bad>"
)
