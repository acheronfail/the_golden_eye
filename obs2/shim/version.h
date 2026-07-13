#ifndef GE_VERSION_H
#define GE_VERSION_H

// The version this shim binary was built with (see GE_SHIM_VERSION in
// cmake/Targets.cmake). The shim is never replaced by auto-update (see reload.c),
// so callers can tell whether a running shim has fallen behind its swapped-in core.
const char *ge_shim_version(void);

#endif
