# Build a distributable plugin archive without making packaging part of the
# default developer/test build.

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

if(CMAKE_SIZEOF_VOID_P EQUAL 8)
  set(GE_PACKAGE_OBS_ARCH_DIR "64bit")
else()
  set(GE_PACKAGE_OBS_ARCH_DIR "32bit")
endif()

set(GE_PACKAGE_BASENAME "${PLUGIN_NAME}-${GE_PACKAGE_PLATFORM}-${GE_PACKAGE_ARCH}")
set(GE_PACKAGE_WORK_DIR "${CMAKE_CURRENT_BINARY_DIR}/package")
set(GE_PACKAGE_STAGE "${GE_PACKAGE_WORK_DIR}/${GE_PACKAGE_BASENAME}")
set(GE_PACKAGE_DIST_DIR "${CMAKE_CURRENT_BINARY_DIR}/dist")
set(GE_PACKAGE_ZIP "${GE_PACKAGE_DIST_DIR}/${GE_PACKAGE_BASENAME}.zip")

if(APPLE)
  set(GE_PACKAGE_ENTRY "${PLUGIN_NAME}.plugin")
  add_custom_target(package-plugin
      COMMAND ${CMAKE_COMMAND} -E rm -rf "${GE_PACKAGE_STAGE}"
      COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_PACKAGE_STAGE}"
      COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_PACKAGE_DIST_DIR}"
      COMMAND ${CMAKE_COMMAND} -E copy_directory
              "$<TARGET_BUNDLE_DIR:${PLUGIN_NAME}>"
              "${GE_PACKAGE_STAGE}/${GE_PACKAGE_ENTRY}"
      COMMAND ${CMAKE_COMMAND} -E rm -f "${GE_PACKAGE_STAGE}/${GE_PACKAGE_ENTRY}/Contents/Resources/cv_templates/.stamp"
      COMMAND ${CMAKE_COMMAND} -E rm -f "${GE_PACKAGE_ZIP}"
      COMMAND ${CMAKE_COMMAND} -E chdir "${GE_PACKAGE_STAGE}"
              ${CMAKE_COMMAND} -E tar cf "${GE_PACKAGE_ZIP}" --format=zip -- "${GE_PACKAGE_ENTRY}"
      COMMAND ${CMAKE_COMMAND} -E echo "Wrote ${GE_PACKAGE_ZIP}"
      COMMENT "Packaging ${GE_PACKAGE_BASENAME}"
      VERBATIM
  )
else()
  set(GE_PACKAGE_ENTRY "${PLUGIN_NAME}")
  set(GE_PACKAGE_PLUGIN_DIR "${GE_PACKAGE_STAGE}/${GE_PACKAGE_ENTRY}")
  set(GE_PACKAGE_BIN_DIR "${GE_PACKAGE_PLUGIN_DIR}/bin/${GE_PACKAGE_OBS_ARCH_DIR}")
  set(GE_PACKAGE_DATA_DIR "${GE_PACKAGE_PLUGIN_DIR}/data")

  add_custom_target(package-plugin
      COMMAND ${CMAKE_COMMAND} -E rm -rf "${GE_PACKAGE_STAGE}"
      COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_PACKAGE_BIN_DIR}"
      COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_PACKAGE_DATA_DIR}/locale"
      COMMAND ${CMAKE_COMMAND} -E make_directory "${GE_PACKAGE_DIST_DIR}"
      COMMAND ${CMAKE_COMMAND} -E copy "$<TARGET_FILE:${PLUGIN_NAME}>" "${GE_PACKAGE_BIN_DIR}/"
      COMMAND ${CMAKE_COMMAND} -E copy "$<TARGET_FILE:${CORE_NAME}>" "${GE_PACKAGE_BIN_DIR}/"
      COMMAND ${CMAKE_COMMAND} -E copy_directory "${GE_BUNDLED_TEMPLATE_DIR}" "${GE_PACKAGE_DATA_DIR}/cv_templates"
      COMMAND ${CMAKE_COMMAND} -E rm -f "${GE_PACKAGE_DATA_DIR}/cv_templates/.stamp"
      COMMAND ${CMAKE_COMMAND} -E rm -f "${GE_PACKAGE_ZIP}"
      COMMAND ${CMAKE_COMMAND} -E chdir "${GE_PACKAGE_STAGE}"
              ${CMAKE_COMMAND} -E tar cf "${GE_PACKAGE_ZIP}" --format=zip -- "${GE_PACKAGE_ENTRY}"
      COMMAND ${CMAKE_COMMAND} -E echo "Wrote ${GE_PACKAGE_ZIP}"
      COMMENT "Packaging ${GE_PACKAGE_BASENAME}"
      VERBATIM
  )
endif()

add_dependencies(package-plugin ${PLUGIN_NAME})
