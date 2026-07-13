#include "version.h"

#ifndef GE_SHIM_VERSION
#error "GE_SHIM_VERSION must be defined by the build"
#endif

const char *ge_shim_version(void) { return GE_SHIM_VERSION; }
