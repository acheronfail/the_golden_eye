// Dev-only hot reload (see dev_reload.h).
//
// The dev build watches the Rust sources and relinks the core library; once
// that succeeds it writes a byte to a FIFO (path shared via the GE_RELOAD_FIFO
// env var). A background thread here blocks reading that FIFO and fires the
// reload callback for each ping — no polling, and we only ever reload after a
// build that actually finished.

#include "dev_reload.h"

#include <obs/libobs/util/base.h>

#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define DR_LOG(level, fmt, ...) blog(level, "[the_golden_eye] " fmt, ##__VA_ARGS__)

static pthread_t g_thread;
static int g_fd = -1;
static volatile bool g_running = false;
static void (*g_on_reload)(void) = NULL;

static const char *fifo_path(void) {
  const char *path = getenv("GE_RELOAD_FIFO");
  return (path && *path) ? path : "/tmp/ge_the_golden_eye.reload";
}

static void *reload_loop(void *arg) {
  (void)arg;
  char buf[64];
  while (g_running) {
    ssize_t n = read(g_fd, buf, sizeof(buf));
    if (n <= 0 || !g_running) {
      break;
    }
    DR_LOG(LOG_INFO, "rebuild signalled — hot reloading core");
    g_on_reload();
  }
  return NULL;
}

void ge_dev_reload_start(void (*on_reload)(void)) {
  g_on_reload = on_reload;
  const char *path = fifo_path();

  if (mkfifo(path, 0600) != 0 && errno != EEXIST) {
    DR_LOG(LOG_WARNING, "could not create reload FIFO '%s': %s; hot reload off", path, strerror(errno));
    return;
  }

  // Open read+write so the FIFO always has a reader (us): reads then block
  // until data arrives instead of hitting EOF, and on stop we can wake the
  // blocked read by writing a byte ourselves.
  g_fd = open(path, O_RDWR);
  if (g_fd < 0) {
    DR_LOG(LOG_WARNING, "could not open reload FIFO '%s': %s; hot reload off", path, strerror(errno));
    return;
  }

  g_running = true;
  if (pthread_create(&g_thread, NULL, reload_loop, NULL) != 0) {
    DR_LOG(LOG_WARNING, "could not start reload thread; hot reload off");
    g_running = false;
    close(g_fd);
    g_fd = -1;
    return;
  }

  DR_LOG(LOG_INFO, "dev hot reload armed (waiting on %s)", path);
}

void ge_dev_reload_stop(void) {
  if (!g_running) {
    return;
  }
  g_running = false;
  // Unblock reload_loop's read() so it sees g_running == false and exits.
  char byte = 0;
  (void)write(g_fd, &byte, 1);
  pthread_join(g_thread, NULL);
  close(g_fd);
  g_fd = -1;
  unlink(fifo_path());
}
