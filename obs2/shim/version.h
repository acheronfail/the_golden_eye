#ifndef GE_VERSION_H
#define GE_VERSION_H

// The version this shim binary was built with, stamped at compile time (see
// GE_SHIM_VERSION in cmake/Targets.cmake). Unlike the core library, the shim
// itself is never replaced by the auto-update flow (see reload.c) -- it keeps
// running the version it was loaded with until OBS restarts -- so callers can
// use this to tell whether a running shim has fallen behind the core it just
// swapped in.
const char *ge_shim_version(void);

#endif
