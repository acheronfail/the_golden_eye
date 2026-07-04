if(NOT DEFINED GE_REQUIRED_FILE)
  message(FATAL_ERROR "GE_REQUIRED_FILE is required")
endif()

if(NOT EXISTS "${GE_REQUIRED_FILE}")
  message(FATAL_ERROR "Required file not found: ${GE_REQUIRED_FILE}")
endif()
