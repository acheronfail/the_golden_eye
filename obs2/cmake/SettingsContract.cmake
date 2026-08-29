# Generate the browser's settings type/defaults from the lightweight Rust
# contract before Vite type-checks or bundles the SPA.

set(SETTINGS_WORKSPACE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/rust")
set(SETTINGS_CONTRACT_DIR "${SETTINGS_WORKSPACE_DIR}/settings")
set(SETTINGS_BINDINGS "${CMAKE_CURRENT_SOURCE_DIR}/browser/src/lib/generated/settings.ts")
file(GLOB_RECURSE SETTINGS_CONTRACT_SOURCES CONFIGURE_DEPENDS
    "${SETTINGS_CONTRACT_DIR}/src/*"
  )

if(GE_REUSE_HOST_BUILD_INPUTS)
  add_custom_target(settings_bindings
      COMMAND ${CMAKE_COMMAND} -E echo "Using existing settings bindings at ${SETTINGS_BINDINGS}"
      COMMAND ${CMAKE_COMMAND}
              "-DGE_REQUIRED_FILE=${SETTINGS_BINDINGS}"
              -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/check-file-exists.cmake"
      VERBATIM
    )
else()
  find_program(SETTINGS_CARGO_EXECUTABLE NAMES cargo REQUIRED)

  add_custom_command(
      OUTPUT "${SETTINGS_BINDINGS}"
      COMMAND ${CMAKE_COMMAND} -E env
              "CARGO_TARGET_DIR=${SETTINGS_WORKSPACE_DIR}/target"
              "${SETTINGS_CARGO_EXECUTABLE}" run
              --quiet
              --locked
              --manifest-path "${SETTINGS_WORKSPACE_DIR}/Cargo.toml"
              --package ge_settings
              --features export
              --bin export-settings
              -- "${SETTINGS_BINDINGS}"
      WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
      DEPENDS
              ${SETTINGS_CONTRACT_SOURCES}
              "${SETTINGS_WORKSPACE_DIR}/Cargo.toml"
              "${SETTINGS_WORKSPACE_DIR}/Cargo.lock"
              "${SETTINGS_CONTRACT_DIR}/Cargo.toml"
      COMMENT "Generating settings contract"
      VERBATIM
    )

  add_custom_target(settings_bindings DEPENDS "${SETTINGS_BINDINGS}")
endif()
