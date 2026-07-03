#ifndef GE_DYNLIB_H
#define GE_DYNLIB_H

#include <stdbool.h>
#include <stddef.h>

#ifdef _WIN32
#include <windows.h>
typedef HMODULE ge_dynlib_handle;
#define GE_PATH_SEP '\\'
#else
typedef void *ge_dynlib_handle;
#define GE_PATH_SEP '/'
#endif

const char *ge_dynlib_error(void);
ge_dynlib_handle ge_dynlib_open(const char *path);
void *ge_dynlib_symbol(ge_dynlib_handle handle, const char *name);
void ge_dynlib_close(ge_dynlib_handle handle);
bool ge_module_path(char *out, size_t out_size);

#endif
