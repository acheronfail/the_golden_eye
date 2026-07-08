if(NOT GE_STRIP_ENABLED)
  return()
endif()

if(NOT GE_STRIP_TOOL)
  return()
endif()

if(NOT GE_STRIP_FILE)
  message(FATAL_ERROR "GE_STRIP_FILE is required")
endif()

if(NOT EXISTS "${GE_STRIP_FILE}")
  message(FATAL_ERROR "Cannot strip missing file: ${GE_STRIP_FILE}")
endif()

execute_process(
    COMMAND "${GE_STRIP_TOOL}" ${GE_STRIP_ARGS} "${GE_STRIP_FILE}"
    RESULT_VARIABLE _strip_result
)
if(NOT _strip_result EQUAL 0)
  message(FATAL_ERROR "Failed to strip ${GE_STRIP_FILE}")
endif()
