# Generate browser API types from the Rust wire contract before Vite checks the SPA.

set(API_WORKSPACE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/rust")
set(API_CORE_DIR "${API_WORKSPACE_DIR}/core")
set(API_BINDINGS "${CMAKE_CURRENT_SOURCE_DIR}/browser/src/lib/generated/api.ts")
file(GLOB_RECURSE API_CONTRACT_SOURCES CONFIGURE_DEPENDS
    "${API_WORKSPACE_DIR}/catalog/src/*"
    "${API_WORKSPACE_DIR}/clip/src/*"
    "${API_CORE_DIR}/src/*"
    "${API_WORKSPACE_DIR}/cv/src/*"
    "${API_WORKSPACE_DIR}/game/src/*"
    "${API_WORKSPACE_DIR}/settings/src/*"
  )

if(GE_REUSE_HOST_BUILD_INPUTS)
  add_custom_target(api_bindings
      COMMAND ${CMAKE_COMMAND} -E echo "Using existing API bindings at ${API_BINDINGS}"
      COMMAND ${CMAKE_COMMAND}
              "-DGE_REQUIRED_FILE=${API_BINDINGS}"
              -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/check-file-exists.cmake"
      VERBATIM
    )
else()
  find_program(API_CARGO_EXECUTABLE NAMES cargo REQUIRED)

  add_custom_command(
      OUTPUT "${API_BINDINGS}"
      COMMAND ${CMAKE_COMMAND} -E env
              "BROWSER_BUNDLE=${CMAKE_CURRENT_SOURCE_DIR}/templates/browser-dev.html.in"
              "GE_PLUGIN_VERSION=${GE_PLUGIN_VERSION}"
              "GE_UPDATER_VERSION=${GE_UPDATER_VERSION}"
              ${RUST_BUILD_ENV}
              "${API_CARGO_EXECUTABLE}" run
              --quiet
              --locked
              --manifest-path "${API_WORKSPACE_DIR}/Cargo.toml"
              --package ge_rust
              --features export
              --bin export-api
              -- "${API_BINDINGS}"
      WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
      DEPENDS
              ${API_CONTRACT_SOURCES}
              "${API_WORKSPACE_DIR}/Cargo.toml"
              "${API_WORKSPACE_DIR}/Cargo.lock"
              "${API_CORE_DIR}/Cargo.toml"
      COMMENT "Generating API contract"
      VERBATIM
    )

  add_custom_target(api_bindings DEPENDS "${API_BINDINGS}")
endif()
