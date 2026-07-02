# Locate the OBS libraries (libobs + obs-frontend-api) to link against.

# Vendored OBS headers (populated by `just obs-headers`)
set(VENDOR_OBS_DIR "${CMAKE_CURRENT_SOURCE_DIR}/vendor/obs")
set(VENDOR_LIBOBS_DIR "${VENDOR_OBS_DIR}/libobs")
set(VENDOR_FRONTEND_DIR "${VENDOR_OBS_DIR}/frontend")

if(NOT EXISTS "${VENDOR_LIBOBS_DIR}/obs-module.h")
  message(FATAL_ERROR "Vendored OBS headers not found. Run 'just obs-headers' first.")
endif()

find_path(GE_SIMDE_INCLUDE_DIR
    NAMES simde/x86/sse2.h
    HINTS /app/include /opt/homebrew/opt/simde/include
)
if(GE_SIMDE_INCLUDE_DIR)
  message(STATUS "Using simde headers from ${GE_SIMDE_INCLUDE_DIR}")
endif()

# Generate obsconfig.h from the vendored template
set(OBS_RELEASE_CANDIDATE 0)
set(OBS_BETA 0)
configure_file(
    "${VENDOR_LIBOBS_DIR}/obsconfig.h.in"
    "${CMAKE_CURRENT_BINARY_DIR}/obsconfig.h"
)

find_package(PkgConfig REQUIRED)

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
  pkg_check_modules(OBS REQUIRED libobs)
  set(OBS_LIBRARIES ${OBS_LDFLAGS})

  # Arch exposes obs-frontend-api via pkg-config, but Debian-based distros
  # often do not. Fall back to finding the shared library directly.
  pkg_check_modules(OBS_FRONTEND QUIET obs-frontend-api)
  if(OBS_FRONTEND_FOUND)
    set(OBS_FRONTEND_LIBRARIES ${OBS_FRONTEND_LDFLAGS})
  else()
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
