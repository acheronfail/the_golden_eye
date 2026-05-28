#include <obs/libobs/obs-module.h>
#include "http.h"

OBS_DECLARE_MODULE()

bool obs_module_load(void) {
  if (!ge_http_server_start()) {
    return false;
  }

  return true;
}

void obs_module_unload(void) {
  ge_http_server_stop();
}