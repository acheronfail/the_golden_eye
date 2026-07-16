# Frontend (SvelteKit SPA) build.
#
# Defines the BROWSER_DEV option and the `browser_build` target, and exports
# BROWSER_BUNDLE — the HTML file the Rust crate embeds via include_str!.
# Must be included after OpenCVStatic, since the prod frontend build command
# forwards RUST_BUILD_ENV to npm.

set(BROWSER_DIR "${CMAKE_CURRENT_SOURCE_DIR}/browser")

# In dev mode we don't build/embed the real SPA. Instead the plugin embeds a
# tiny HTML file that redirects to the Vite dev server, so the frontend can be
# iterated on with hot reloads while only the plugin needs CMake rebuilds.
option(BROWSER_DEV "Embed a dev-server redirect instead of building the SPA" OFF)

# Port the Vite dev server listens on (must match browser/vite.config.ts).
set(BROWSER_DEV_PORT 5173)

if(BROWSER_DEV)
  # Generate the redirect stand-in and point the embedded bundle at it.
  set(BROWSER_BUNDLE "${CMAKE_CURRENT_BINARY_DIR}/browser-dev.html")
  configure_file(
        "${CMAKE_CURRENT_SOURCE_DIR}/templates/browser-dev.html.in"
        "${BROWSER_BUNDLE}"
        @ONLY
    )
  # No SPA build in dev mode; keep the target so rust_build's dependency
  # resolves, but make it a no-op.
  add_custom_target(browser_build
        COMMENT "Skipping frontend build (BROWSER_DEV=ON; using dev-server redirect)"
    )
else()
  # The embedded bundle path. The justfile is the single source of truth and
  # exports BROWSER_BUNDLE, so honour it from the environment when present; fall
  # back to the standard build output when CMake is invoked directly (without
  # the justfile env). Passed on to the frontend build (svelte.config.js reads
  # $BROWSER_BUNDLE to decide its output location) and to the Rust build
  # (include_str!(env!("BROWSER_BUNDLE"))).
  if(DEFINED ENV{BROWSER_BUNDLE})
    set(BROWSER_BUNDLE "$ENV{BROWSER_BUNDLE}")
  else()
    set(BROWSER_BUNDLE "${BROWSER_DIR}/build/index.html")
  endif()

  # Track frontend sources and key build inputs so the bundle only rebuilds
  # when one of them is newer than the generated HTML.
  file(GLOB_RECURSE BROWSER_BUILD_DEPENDS CONFIGURE_DEPENDS
        "${BROWSER_DIR}/src/*"
        "${BROWSER_DIR}/static/*"
    )
  list(APPEND BROWSER_BUILD_DEPENDS
        "${BROWSER_DIR}/package.json"
        "${BROWSER_DIR}/package-lock.json"
        "${BROWSER_DIR}/svelte.config.js"
        "${BROWSER_DIR}/tsconfig.json"
        "${BROWSER_DIR}/vite.config.ts"
    )

  if(GE_REUSE_HOST_BUILD_INPUTS)
    add_custom_target(browser_build ALL
          COMMAND ${CMAKE_COMMAND} -E echo "Using existing browser bundle at ${BROWSER_BUNDLE}"
          COMMAND ${CMAKE_COMMAND}
                  "-DGE_REQUIRED_FILE=${BROWSER_BUNDLE}"
                  -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/check-file-exists.cmake"
          VERBATIM
      )
  else()
    if(WIN32)
      find_program(NPM_EXECUTABLE NAMES npm.cmd npm REQUIRED)
    else()
      find_program(NPM_EXECUTABLE NAMES npm REQUIRED)
    endif()

    add_custom_command(
          OUTPUT "${BROWSER_BUNDLE}"
          COMMAND ${CMAKE_COMMAND} -E env
                  "BROWSER_BUNDLE=${BROWSER_BUNDLE}"
                  ${RUST_BUILD_ENV}
                  "${NPM_EXECUTABLE}" run build
          # Fail the build if the bundle the Rust crate embeds wasn't produced,
          # rather than letting cargo fail later with an opaque include_str! error.
          COMMAND ${CMAKE_COMMAND}
                  "-DGE_REQUIRED_FILE=${BROWSER_BUNDLE}"
                  -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/check-file-exists.cmake"
          WORKING_DIRECTORY "${BROWSER_DIR}"
          DEPENDS ${BROWSER_BUILD_DEPENDS}
          COMMENT "Building frontend"
          VERBATIM
      )

    add_custom_target(browser_build ALL
          DEPENDS "${BROWSER_BUNDLE}"
      )
  endif()
endif()
