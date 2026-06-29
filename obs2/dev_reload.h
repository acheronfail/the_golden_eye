#ifndef GE_DEV_RELOAD_H
#define GE_DEV_RELOAD_H

// Dev-only hot reload. Rather than polling the core library on disk, the shim
// blocks on a FIFO; the dev build (`just dev`) writes to it after each
// successful rebuild, at which point `on_reload` is invoked. Compiled into the
// shim only when BROWSER_DEV=ON. See dev_reload.c.

void ge_dev_reload_start(void (*on_reload)(void));
void ge_dev_reload_stop(void);

#endif /* GE_DEV_RELOAD_H */
