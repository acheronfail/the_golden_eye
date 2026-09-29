// The UI adapter's retained image must survive the loader releasing its handle.
#include "../dynlib.h"
#include <stdio.h>

int main(int argc, char **argv) {
  if (argc != 2) {
    return 2;
  }
  ge_dynlib_handle library = ge_dynlib_open(argv[1]);
  if (!library) {
    fprintf(stderr, "%s\n", ge_dynlib_error());
    return 1;
  }
  bool (*pin)(void) = (bool (*)(void))ge_dynlib_symbol(library, "ge_obs_pin_core_for_ui");
  if (!pin || !pin()) {
    ge_dynlib_close(library);
    return 1;
  }
  ge_dynlib_close(library);
  // Calling into the old image models a queued callback after runtime teardown.
  return pin() ? 0 : 1;
}
