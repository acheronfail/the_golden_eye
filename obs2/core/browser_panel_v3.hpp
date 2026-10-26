#pragma once

#include <obs/libobs/util/util.hpp>
#include <string>

// OBS Browser's public QCef v3 interface, with unused Qt types forward declared.
// https://github.com/obsproject/obs-browser/blob/5b503289c45a/panel/browser-panel.hpp
class QWidget;
class QObject;
class QCefWidget;
struct QCefCookieManager;

struct QCef {
  virtual ~QCef() {}
  virtual bool init_browser(void) = 0;
  virtual bool initialized(void) = 0;
  virtual bool wait_for_browser_init(void) = 0;
  virtual QCefWidget *create_widget(QWidget *parent, const std::string &url,
                                    QCefCookieManager *cookie_manager = nullptr) = 0;
  virtual QCefCookieManager *create_cookie_manager(const std::string &storage_path,
                                                   bool persist_session_cookies = false) = 0;
  virtual BPtr<char> get_cookie_path(const std::string &storage_path) = 0;
  virtual void add_popup_whitelist_url(const std::string &url, QObject *obj) = 0;
  virtual void add_force_popup_url(const std::string &url, QObject *obj) = 0;
};
