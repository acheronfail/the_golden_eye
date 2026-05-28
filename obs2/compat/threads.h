/* Minimal C11 threads shim for Apple Clang, which doesn't ship threads.h.
 * Only the symbols used by this project are implemented. */
#pragma once
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>

typedef pthread_t thrd_t;
typedef int (*thrd_start_t)(void *);

enum {
  thrd_success = 0,
  thrd_nomem = 1,
  thrd_timedout = 2,
  thrd_busy = 3,
  thrd_error = 4,
};

/* C11 thread funcs return int; pthreads return void* — bridge the gap. */
typedef struct {
  thrd_start_t func;
  void *arg;
} _thrd_args;

static inline void *_thrd_wrapper(void *arg) {
  _thrd_args *a = (_thrd_args *)arg;
  int r = a->func(a->arg);
  free(a);
  return (void *)(intptr_t)r;
}

static inline int thrd_create(thrd_t *thr, thrd_start_t func, void *arg) {
  _thrd_args *a = malloc(sizeof(*a));
  if (!a)
    return thrd_nomem;
  a->func = func;
  a->arg = arg;
  if (pthread_create(thr, NULL, _thrd_wrapper, a) != 0) {
    free(a);
    return thrd_error;
  }
  return thrd_success;
}

static inline int thrd_join(thrd_t thr, int *res) {
  void *retval;
  if (pthread_join(thr, &retval) != 0)
    return thrd_error;
  if (res)
    *res = (int)(intptr_t)retval;
  return thrd_success;
}
