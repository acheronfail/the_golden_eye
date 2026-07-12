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

add_executable(ge_shim_reload_tests
    ../dynlib.c
    ../reload.c
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
