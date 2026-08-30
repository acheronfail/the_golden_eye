# Generate the browser's settings and API contracts before Vite checks the SPA.

set(CONTRACTS_WORKSPACE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/rust")
set(CONTRACTS_SETTINGS_DIR "${CONTRACTS_WORKSPACE_DIR}/settings")
set(CONTRACTS_CORE_DIR "${CONTRACTS_WORKSPACE_DIR}/core")
set(SETTINGS_BINDINGS "${CMAKE_CURRENT_SOURCE_DIR}/browser/src/lib/generated/settings.ts")
set(API_BINDINGS "${CMAKE_CURRENT_SOURCE_DIR}/browser/src/lib/generated/api.ts")

file(GLOB_RECURSE SETTINGS_CONTRACT_SOURCES CONFIGURE_DEPENDS
    "${CONTRACTS_SETTINGS_DIR}/src/*"
  )
file(GLOB_RECURSE API_CONTRACT_SOURCES CONFIGURE_DEPENDS
    "${CONTRACTS_WORKSPACE_DIR}/catalog/src/*"
    "${CONTRACTS_WORKSPACE_DIR}/clip/src/*"
    "${CONTRACTS_CORE_DIR}/src/*"
    "${CONTRACTS_WORKSPACE_DIR}/cv/src/*"
    "${CONTRACTS_WORKSPACE_DIR}/game/src/*"
  )

if(GE_REUSE_HOST_BUILD_INPUTS)
  add_custom_target(browser_contracts
      COMMAND ${CMAKE_COMMAND} -E echo "Using existing browser contracts"
      COMMAND ${CMAKE_COMMAND}
              "-DGE_REQUIRED_FILE=${SETTINGS_BINDINGS}"
              -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/check-file-exists.cmake"
      COMMAND ${CMAKE_COMMAND}
              "-DGE_REQUIRED_FILE=${API_BINDINGS}"
              -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/check-file-exists.cmake"
      VERBATIM
    )
else()
  find_program(CONTRACTS_CARGO_EXECUTABLE NAMES cargo REQUIRED)

  add_custom_command(
      OUTPUT "${SETTINGS_BINDINGS}"
      COMMAND ${CMAKE_COMMAND} -E env
              "CARGO_TARGET_DIR=${CONTRACTS_WORKSPACE_DIR}/target"
              "${CONTRACTS_CARGO_EXECUTABLE}" run
              --quiet
              --locked
              --manifest-path "${CONTRACTS_WORKSPACE_DIR}/Cargo.toml"
              --package ge_settings
              --bin export-settings
              -- "${SETTINGS_BINDINGS}"
      WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
      DEPENDS
              ${SETTINGS_CONTRACT_SOURCES}
              "${CONTRACTS_WORKSPACE_DIR}/Cargo.toml"
              "${CONTRACTS_WORKSPACE_DIR}/Cargo.lock"
              "${CONTRACTS_SETTINGS_DIR}/Cargo.toml"
      COMMENT "Generating settings contract"
      VERBATIM
    )

  add_custom_command(
      OUTPUT "${API_BINDINGS}"
      COMMAND ${CMAKE_COMMAND} -E env
              "BROWSER_BUNDLE=${CMAKE_CURRENT_SOURCE_DIR}/templates/browser-dev.html.in"
              "GE_PLUGIN_VERSION=${GE_PLUGIN_VERSION}"
              "GE_UPDATER_VERSION=${GE_UPDATER_VERSION}"
              ${RUST_BUILD_ENV}
              "${CONTRACTS_CARGO_EXECUTABLE}" run
              --quiet
              --locked
              --manifest-path "${CONTRACTS_WORKSPACE_DIR}/Cargo.toml"
              --package ge_rust
              --bin export-api
              -- "${API_BINDINGS}"
      WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
      DEPENDS
              ${API_CONTRACT_SOURCES}
              "${CONTRACTS_WORKSPACE_DIR}/Cargo.toml"
              "${CONTRACTS_WORKSPACE_DIR}/Cargo.lock"
              "${CONTRACTS_CORE_DIR}/Cargo.toml"
      COMMENT "Generating API contract"
      VERBATIM
    )

  add_custom_target(browser_contracts DEPENDS "${SETTINGS_BINDINGS}" "${API_BINDINGS}")
endif()
