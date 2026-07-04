# Locate the OBS libraries (libobs + obs-frontend-api) to link against.
#
# Linux development normally builds the final plugin inside the OBS Flatpak SDK,
# but the host build tree is still configured first so it can build the browser
# bundle and Rust staticlib. Allow that host configure to proceed without host
# OBS development files; targets that actually produce native OBS shared
# libraries add a clear build-time guard in Targets.cmake.

# Vendored OBS headers (populated by `just obs-headers`)
set(VENDOR_OBS_DIR "${CMAKE_CURRENT_SOURCE_DIR}/vendor/obs")
set(VENDOR_LIBOBS_DIR "${VENDOR_OBS_DIR}/libobs")
set(VENDOR_FRONTEND_DIR "${VENDOR_OBS_DIR}/frontend")

if(NOT EXISTS "${VENDOR_LIBOBS_DIR}/obs-module.h")
  message(FATAL_ERROR "Vendored OBS headers not found. Run 'just obs-headers' first.")
endif()

option(GE_LINUX_NATIVE_OBS_BUILD "Configure native Linux OBS plugin targets against the local OBS SDK" OFF)
set(GE_WINDOWS_OBS_ROOT "" CACHE PATH "Windows OBS Studio install root (defaults to Program Files/obs-studio)")

set(GE_OBS_NATIVE_DEPS_FOUND TRUE)
set(GE_OBS_NATIVE_DEPS_ERRORS "")
set(GE_OBS_DYNAMIC_LOOKUP FALSE)
set(OBS_LIBRARIES "")
set(OBS_FRONTEND_LIBRARIES "")

# Generate obsconfig.h from the vendored template. This is cheap and useful for
# editor tooling even when the host Linux configure intentionally skips native
# OBS library discovery.
set(OBS_RELEASE_CANDIDATE 0)
set(OBS_BETA 0)
configure_file(
    "${VENDOR_LIBOBS_DIR}/obsconfig.h.in"
    "${CMAKE_CURRENT_BINARY_DIR}/obsconfig.h"
)

function(ge_make_windows_import_lib target_name dll_path out_lib)
  set(_ge_msvc_tool_hints "")
  if(WIN32 AND CMAKE_C_COMPILER)
    get_filename_component(_ge_c_compiler_dir "${CMAKE_C_COMPILER}" DIRECTORY)
    list(APPEND _ge_msvc_tool_hints "${_ge_c_compiler_dir}")
  endif()
  if(WIN32 AND DEFINED ENV{VCToolsInstallDir})
    file(TO_CMAKE_PATH "$ENV{VCToolsInstallDir}" _ge_vc_tools_dir)
    if(CMAKE_SIZEOF_VOID_P EQUAL 8)
      list(APPEND _ge_msvc_tool_hints
          "${_ge_vc_tools_dir}/bin/Hostx64/x64"
          "${_ge_vc_tools_dir}/bin/Hostx86/x64")
    else()
      list(APPEND _ge_msvc_tool_hints
          "${_ge_vc_tools_dir}/bin/Hostx64/x86"
          "${_ge_vc_tools_dir}/bin/Hostx86/x86")
    endif()
  endif()

  find_program(GE_DUMPBIN_EXECUTABLE NAMES dumpbin.exe dumpbin link.exe link HINTS ${_ge_msvc_tool_hints} NO_DEFAULT_PATH)
  find_program(GE_LIB_EXECUTABLE NAMES lib.exe lib HINTS ${_ge_msvc_tool_hints} NO_DEFAULT_PATH)
  if(NOT GE_DUMPBIN_EXECUTABLE OR NOT GE_LIB_EXECUTABLE)
    message(FATAL_ERROR
            "dumpbin.exe or link.exe, plus lib.exe, are required to generate OBS import libraries. "
            "Run CMake from a Visual Studio Developer shell or initialize MSVC first.")
  endif()
  if(CMAKE_SIZEOF_VOID_P EQUAL 8)
    set(_machine "x64")
  else()
    set(_machine "x86")
  endif()

  set(_def_file "${CMAKE_CURRENT_BINARY_DIR}/${target_name}.def")
  execute_process(
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_CURRENT_SOURCE_DIR}/cmake/make-import-lib.ps1"
              -DllPath "${dll_path}"
              -DefPath "${_def_file}"
              -LibPath "${out_lib}"
              -Machine "${_machine}"
              -Dumpbin "${GE_DUMPBIN_EXECUTABLE}"
              -Lib "${GE_LIB_EXECUTABLE}"
      RESULT_VARIABLE _import_result
  )
  if(NOT _import_result EQUAL 0)
    message(FATAL_ERROR "Failed to generate import library for ${dll_path}")
  endif()
endfunction()

if(UNIX AND NOT APPLE AND NOT GE_LINUX_NATIVE_OBS_BUILD)
  # Linux artifacts target the OBS Flatpak SDK. The host build tree is used for
  # frontend/Rust inputs only, so do not probe or link against an arbitrary host
  # OBS installation even if one happens to be present.
  set(GE_OBS_NATIVE_DEPS_FOUND FALSE)
  set(GE_SIMDE_INCLUDE_DIR "")
else()
  if(DEFINED GE_SIMDE_INCLUDE_DIR AND GE_SIMDE_INCLUDE_DIR STREQUAL "")
    unset(GE_SIMDE_INCLUDE_DIR CACHE)
    unset(GE_SIMDE_INCLUDE_DIR)
  endif()
  find_path(GE_SIMDE_INCLUDE_DIR
      NAMES simde/x86/sse2.h
      HINTS /app/include /opt/homebrew/opt/simde/include
  )
  if(GE_SIMDE_INCLUDE_DIR)
    message(STATUS "Using simde headers from ${GE_SIMDE_INCLUDE_DIR}")
  else()
    set(GE_OBS_NATIVE_DEPS_FOUND FALSE)
    list(APPEND GE_OBS_NATIVE_DEPS_ERRORS "simde headers")
    set(GE_SIMDE_INCLUDE_DIR "")
    if(GE_LINUX_NATIVE_OBS_BUILD OR WIN32)
      if(UNIX AND NOT APPLE)
        set(_GE_SIMDE_HINT "Install simde or configure/build inside the OBS Flatpak SDK.")
      elseif(WIN32)
        set(_GE_SIMDE_HINT "Install simde with vcpkg or pass -DGE_SIMDE_INCLUDE_DIR=...")
      else()
        set(_GE_SIMDE_HINT "Install simde.")
      endif()
      message(FATAL_ERROR
              "Could not find simde headers (simde/x86/sse2.h). ${_GE_SIMDE_HINT}")
    endif()
  endif()

  if(APPLE)
    # macOS OBS plugins are loaded into the OBS process, so unresolved OBS
    # symbols can be resolved by dyld at load time. This lets CI build
    # distributable bundles from vendored OBS headers without installing OBS on
    # the build host.
    set(GE_OBS_DYNAMIC_LOOKUP TRUE)
    message(STATUS "Using macOS dynamic lookup for OBS symbols")
  elseif(WIN32)
    if(NOT GE_WINDOWS_OBS_ROOT)
      if(DEFINED ENV{ProgramFiles})
        file(TO_CMAKE_PATH "$ENV{ProgramFiles}" _GE_PROGRAM_FILES)
      else()
        set(_GE_PROGRAM_FILES "C:/Program Files")
      endif()
      set(GE_WINDOWS_OBS_ROOT "${_GE_PROGRAM_FILES}/obs-studio" CACHE PATH "Windows OBS Studio install root" FORCE)
    endif()

    if(CMAKE_SIZEOF_VOID_P EQUAL 8)
      set(_GE_WINDOWS_OBS_ARCH_DIR "64bit")
    else()
      set(_GE_WINDOWS_OBS_ARCH_DIR "32bit")
    endif()

    find_file(GE_WINDOWS_OBS_DLL
        NAMES obs.dll
        HINTS "${GE_WINDOWS_OBS_ROOT}/bin/${_GE_WINDOWS_OBS_ARCH_DIR}"
        NO_DEFAULT_PATH
    )
    find_file(GE_WINDOWS_OBS_FRONTEND_DLL
        NAMES obs-frontend-api.dll
        HINTS "${GE_WINDOWS_OBS_ROOT}/bin/${_GE_WINDOWS_OBS_ARCH_DIR}"
        NO_DEFAULT_PATH
    )

    if(NOT GE_WINDOWS_OBS_DLL OR NOT GE_WINDOWS_OBS_FRONTEND_DLL)
      set(GE_OBS_NATIVE_DEPS_FOUND FALSE)
      list(APPEND GE_OBS_NATIVE_DEPS_ERRORS "OBS runtime DLLs under ${GE_WINDOWS_OBS_ROOT}/bin/${_GE_WINDOWS_OBS_ARCH_DIR}")
      message(FATAL_ERROR
              "Could not find obs.dll and obs-frontend-api.dll. Install OBS Studio or pass "
              "-DGE_WINDOWS_OBS_ROOT=... pointing at the OBS install root.")
    endif()

    set(_GE_OBS_IMPORT_LIB "${CMAKE_CURRENT_BINARY_DIR}/obs.lib")
    set(_GE_OBS_FRONTEND_IMPORT_LIB "${CMAKE_CURRENT_BINARY_DIR}/obs-frontend-api.lib")
    ge_make_windows_import_lib(obs "${GE_WINDOWS_OBS_DLL}" "${_GE_OBS_IMPORT_LIB}")
    ge_make_windows_import_lib(obs-frontend-api "${GE_WINDOWS_OBS_FRONTEND_DLL}" "${_GE_OBS_FRONTEND_IMPORT_LIB}")
    set(OBS_LIBRARIES "${_GE_OBS_IMPORT_LIB}")
    set(OBS_FRONTEND_LIBRARIES "${_GE_OBS_FRONTEND_IMPORT_LIB}")
  else()
    find_package(PkgConfig REQUIRED)

    pkg_check_modules(OBS QUIET libobs)
    if(OBS_FOUND)
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
        if(OBS_FRONTEND_LIBRARY)
          set(OBS_FRONTEND_LIBRARIES ${OBS_FRONTEND_LIBRARY})
        else()
          set(GE_OBS_NATIVE_DEPS_FOUND FALSE)
          list(APPEND GE_OBS_NATIVE_DEPS_ERRORS "OBS frontend API library")
          set(OBS_FRONTEND_LIBRARIES "")
          if(GE_LINUX_NATIVE_OBS_BUILD)
            message(FATAL_ERROR
                    "Could not find OBS frontend API library. Install OBS development files "
                    "or configure/build inside the OBS Flatpak SDK.")
          endif()
        endif()
      endif()
    else()
      set(GE_OBS_NATIVE_DEPS_FOUND FALSE)
      list(APPEND GE_OBS_NATIVE_DEPS_ERRORS "libobs pkg-config module")
      set(OBS_LIBRARIES "")
      set(OBS_FRONTEND_LIBRARIES "")
      if(GE_LINUX_NATIVE_OBS_BUILD)
        message(FATAL_ERROR
                "Could not find libobs via pkg-config. Install OBS development files "
                "or configure/build inside the OBS Flatpak SDK.")
      endif()
    endif()
  endif()

  if(GE_LINUX_NATIVE_OBS_BUILD AND NOT GE_OBS_NATIVE_DEPS_FOUND)
    string(REPLACE ";" ", " GE_OBS_NATIVE_DEPS_ERROR_TEXT "${GE_OBS_NATIVE_DEPS_ERRORS}")
    message(WARNING
            "Some native OBS build dependencies were not found "
            "(${GE_OBS_NATIVE_DEPS_ERROR_TEXT}). Non-native targets such as "
            "browser_build and rust_build can still be used; native OBS plugin "
            "targets must be built inside the OBS Flatpak SDK or on a host with "
            "OBS development files installed.")
  endif()
endif()
