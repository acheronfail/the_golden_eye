#include "version.h"

#ifndef GE_LOADER_VERSION
#error "GE_LOADER_VERSION must be defined by the build"
#endif

const char *ge_loader_version(void) { return GE_LOADER_VERSION; }
