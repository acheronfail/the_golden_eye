# Build, install, and uninstall distributable plugin artifacts without making
# packaging or installation part of the default developer/test build.

if(APPLE)
  set(GE_PACKAGE_PLATFORM "macos")
elseif(WIN32)
  set(GE_PACKAGE_PLATFORM "windows")
elseif(UNIX)
  set(GE_PACKAGE_PLATFORM "linux")
else()
  message(FATAL_ERROR "Unsupported package platform: ${CMAKE_SYSTEM_NAME}")
endif()

if(CMAKE_SYSTEM_PROCESSOR)
  set(GE_PACKAGE_ARCH_RAW "${CMAKE_SYSTEM_PROCESSOR}")
else()
  set(GE_PACKAGE_ARCH_RAW "${CMAKE_HOST_SYSTEM_PROCESSOR}")
endif()
string(TOLOWER "${GE_PACKAGE_ARCH_RAW}" GE_PACKAGE_ARCH)
if(GE_PACKAGE_ARCH STREQUAL "amd64")
  set(GE_PACKAGE_ARCH "x86_64")
elseif(GE_PACKAGE_ARCH STREQUAL "aarch64")
  set(GE_PACKAGE_ARCH "arm64")
endif()

set(GE_PACKAGE_OBS_ARCH_DIR "${GE_OBS_ARCH_DIR}")

set(GE_PACKAGE_BASENAME "${PLUGIN_NAME}-${GE_PACKAGE_PLATFORM}-${GE_PACKAGE_ARCH}")
set(GE_PACKAGE_WORK_DIR "${CMAKE_CURRENT_BINARY_DIR}/package")
set(GE_PACKAGE_STAGE "${GE_PACKAGE_WORK_DIR}/${GE_PACKAGE_BASENAME}")
set(GE_PACKAGE_DIST_DIR "${CMAKE_CURRENT_BINARY_DIR}/dist")
set(GE_PACKAGE_ZIP "${GE_PACKAGE_DIST_DIR}/${GE_PACKAGE_BASENAME}.zip")

if(NOT DEFINED GE_PLUGIN_INSTALL_ROOT)
  if(APPLE)
    if(NOT DEFINED ENV{HOME})
      message(FATAL_ERROR
          "HOME is required to determine the OBS plugin install root. "
          "Pass -DGE_PLUGIN_INSTALL_ROOT=... to override."
      )
    endif()
    set(_GE_DEFAULT_PLUGIN_INSTALL_ROOT "$ENV{HOME}/Library/Application Support/obs-studio/plugins")
  elseif(WIN32)
    if(DEFINED ENV{ProgramData})
      file(TO_CMAKE_PATH "$ENV{ProgramData}" _GE_WINDOWS_PROGRAM_DATA)
    else()
      set(_GE_WINDOWS_PROGRAM_DATA "C:/ProgramData")
    endif()
    set(_GE_DEFAULT_PLUGIN_INSTALL_ROOT "${_GE_WINDOWS_PROGRAM_DATA}/obs-studio/plugins")
  else()
    if(NOT DEFINED ENV{HOME})
      message(FATAL_ERROR
          "HOME is required to determine the OBS plugin install root. "
          "Pass -DGE_PLUGIN_INSTALL_ROOT=... to override."
      )
    endif()
    set(_GE_DEFAULT_PLUGIN_INSTALL_ROOT "$ENV{HOME}/.var/app/com.obsproject.Studio/config/obs-studio/plugins")
  endif()
  set(GE_PLUGIN_INSTALL_ROOT "${_GE_DEFAULT_PLUGIN_INSTALL_ROOT}" CACHE PATH "OBS plugin install root")
endif()

if(APPLE)
  set(GE_PACKAGE_ENTRY "${PLUGIN_NAME}.plugin")
  set(GE_PACKAGE_ENTRY_PATH "${GE_PACKAGE_STAGE}/${GE_PACKAGE_ENTRY}")

  add_custom_target(stage-plugin
      COMMAND ${CMAKE_COMMAND} -E rm -rf "${GE_PACKAGE_STAGE}"
      COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_PACKAGE_STAGE}"
      COMMAND ${CMAKE_COMMAND} -E copy_directory
              "$<TARGET_BUNDLE_DIR:${PLUGIN_NAME}>"
              "${GE_PACKAGE_ENTRY_PATH}"
      COMMAND ${CMAKE_COMMAND} -E copy
              "${GE_PLUGIN_VERSION_FILE}"
              "${GE_PACKAGE_ENTRY_PATH}/VERSION"
      COMMAND ${CMAKE_COMMAND} -E rm -f "${GE_PACKAGE_ENTRY_PATH}/Contents/Resources/cv_templates/.stamp"
      COMMENT "Staging ${GE_PACKAGE_BASENAME}"
      VERBATIM
  )
else()
  set(GE_PACKAGE_ENTRY "${PLUGIN_NAME}")
  set(GE_PACKAGE_ENTRY_PATH "${GE_PACKAGE_STAGE}/${GE_PACKAGE_ENTRY}")
  set(GE_PACKAGE_BIN_DIR "${GE_PACKAGE_ENTRY_PATH}/bin/${GE_PACKAGE_OBS_ARCH_DIR}")
  set(GE_PACKAGE_DATA_DIR "${GE_PACKAGE_ENTRY_PATH}/data")
  set(GE_PACKAGE_LOCALE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/locale")

  add_custom_target(stage-plugin
      COMMAND ${CMAKE_COMMAND} -E rm -rf "${GE_PACKAGE_STAGE}"
      COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_PACKAGE_BIN_DIR}"
      COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_PACKAGE_DATA_DIR}/locale"
      COMMAND ${CMAKE_COMMAND} -E copy "$<TARGET_FILE:${PLUGIN_NAME}>" "${GE_PACKAGE_BIN_DIR}/"
      COMMAND ${CMAKE_COMMAND} -E copy "$<TARGET_FILE:${CORE_NAME}>" "${GE_PACKAGE_BIN_DIR}/"
      COMMAND ${CMAKE_COMMAND} -E copy_directory "${GE_PACKAGE_LOCALE_DIR}" "${GE_PACKAGE_DATA_DIR}/locale"
      COMMAND ${CMAKE_COMMAND} -E copy_directory "${GE_BUNDLED_TEMPLATE_DIR}" "${GE_PACKAGE_DATA_DIR}/cv_templates"
      COMMAND ${CMAKE_COMMAND} -E copy "${GE_PLUGIN_VERSION_FILE}" "${GE_PACKAGE_ENTRY_PATH}/VERSION"
      COMMAND ${CMAKE_COMMAND} -E rm -f "${GE_PACKAGE_DATA_DIR}/cv_templates/.stamp"
      COMMENT "Staging ${GE_PACKAGE_BASENAME}"
      VERBATIM
  )
endif()

add_dependencies(stage-plugin ${PLUGIN_NAME})

add_custom_target(package-plugin
    COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_PACKAGE_DIST_DIR}"
    COMMAND ${CMAKE_COMMAND} -E rm -f "${GE_PACKAGE_ZIP}"
    COMMAND ${CMAKE_COMMAND} -E chdir "${GE_PACKAGE_STAGE}"
            ${CMAKE_COMMAND} -E tar cf "${GE_PACKAGE_ZIP}" --format=zip -- "${GE_PACKAGE_ENTRY}"
    COMMAND ${CMAKE_COMMAND} -E echo "Wrote ${GE_PACKAGE_ZIP}"
    COMMENT "Packaging ${GE_PACKAGE_BASENAME}"
    VERBATIM
)
add_dependencies(package-plugin stage-plugin)

add_custom_target(install-plugin
    COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_PLUGIN_INSTALL_ROOT}"
    COMMAND ${CMAKE_COMMAND} -E rm -rf "${GE_PLUGIN_INSTALL_ROOT}/${GE_PACKAGE_ENTRY}"
    COMMAND ${CMAKE_COMMAND} -E copy_directory
            "${GE_PACKAGE_ENTRY_PATH}"
            "${GE_PLUGIN_INSTALL_ROOT}/${GE_PACKAGE_ENTRY}"
    COMMAND ${CMAKE_COMMAND} -E echo "Installed ${GE_PACKAGE_ENTRY} to ${GE_PLUGIN_INSTALL_ROOT}"
    COMMENT "Installing ${GE_PACKAGE_ENTRY}"
    VERBATIM
)
add_dependencies(install-plugin stage-plugin)

add_custom_target(uninstall-plugin
    COMMAND ${CMAKE_COMMAND} -E rm -rf "${GE_PLUGIN_INSTALL_ROOT}/${GE_PACKAGE_ENTRY}"
    COMMAND ${CMAKE_COMMAND} -E echo "Uninstalled ${GE_PACKAGE_ENTRY} from ${GE_PLUGIN_INSTALL_ROOT}"
    COMMENT "Uninstalling ${GE_PACKAGE_ENTRY}"
    VERBATIM
)
