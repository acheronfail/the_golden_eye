#ifndef GE_VERSION_H
#define GE_VERSION_H

// The version this loader binary was built with (see GE_LOADER_VERSION in
// cmake/Targets.cmake). The loader is never replaced by auto-update (see reload.c),
// so callers can tell whether a running loader has fallen behind its swapped-in core.
const char *ge_loader_version(void);

#endif
