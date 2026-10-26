#include "browser_panel_v3.hpp"
#include "obs_bridge.h"

#include <chrono>
#include <exception>
#include <memory>
#include <obs/libobs/obs-module.h>
#include <obs/libobs/util/platform.h>
#include <thread>

bool ge_obs_prepare_browser_dock(void) try {
  if ((obs_get_version() >> 24) < 33) {
    return true;
  }
  obs_module_t *browser = obs_get_module("obs-browser");
  if (!browser) {
    return true;
  }
  void *library = obs_get_module_lib(browser);
  if (!library) {
    return false;
  }
  auto version = reinterpret_cast<int (*)(void)>(os_dlsym(library, "obs_browser_qcef_version_export"));
  auto create = reinterpret_cast<QCef *(*)(void)>(os_dlsym(library, "obs_browser_create_qcef"));
  if (!version || version() != 3 || !create) {
    blog(LOG_WARNING, "[the_golden_eye] unknown browser panel interface; skipping browser pre-initialization");
    return true;
  }
  std::unique_ptr<QCef> panel(create());
  if (!panel) {
    return false;
  }
  panel->init_browser();
  const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(10);
  while (!panel->initialized()) {
    if (std::chrono::steady_clock::now() >= deadline) {
      blog(LOG_WARNING, "[the_golden_eye] browser initialization timed out; not registering a new dock");
      return false;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
  }
  blog(LOG_INFO, "[the_golden_eye] browser initialized before dock registration");
  return true;
} catch (const std::exception &error) {
  blog(LOG_WARNING, "[the_golden_eye] browser initialization failed: %s", error.what());
  return false;
} catch (...) {
  blog(LOG_WARNING, "[the_golden_eye] browser initialization failed with an unknown exception");
  return false;
}
