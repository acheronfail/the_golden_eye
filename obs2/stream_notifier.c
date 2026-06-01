#include "stream_notifier.h"
#include "logger.h"
#include "vendor/mongoose.h"
#include <obs/libobs/obs-module.h>

#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// TODO: Use OS keyring
static char *get_tokens_path(void) { return obs_module_config_path("youtube_oauth_tokens.json"); }

struct oauth_ctx {
  char code[512];
  int done;
};

static char *read_file(const char *path) {
  FILE *fp = fopen(path, "r");
  if (!fp)
    return NULL;
  fseek(fp, 0, SEEK_END);
  long size = ftell(fp);
  fseek(fp, 0, SEEK_SET);
  if (size <= 0) {
    fclose(fp);
    return NULL;
  }
  char *buf = malloc(size + 1);
  if (!buf) {
    fclose(fp);
    return NULL;
  }
  size_t read_bytes = fread(buf, 1, size, fp);
  buf[read_bytes] = '\0';
  fclose(fp);
  return buf;
}

static void write_file(const char *path, const char *content) {
  FILE *fp = fopen(path, "w");
  if (fp) {
    fputs(content, fp);
    fclose(fp);
  }
}

// TODO: Use libcurl
static char *run_curl(const char *cmd) {
  ge_log_info("Executing curl API request...");
  FILE *fp = popen(cmd, "r");
  if (!fp)
    return NULL;

  size_t size = 4096;
  char *buffer = malloc(size);
  if (!buffer) {
    pclose(fp);
    return NULL;
  }
  size_t len = 0;

  char chunk[512];
  while (fgets(chunk, sizeof(chunk), fp) != NULL) {
    size_t chunk_len = strlen(chunk);
    if (len + chunk_len >= size) {
      size *= 2;
      char *new_buf = realloc(buffer, size);
      if (!new_buf) {
        free(buffer);
        pclose(fp);
        return NULL;
      }
      buffer = new_buf;
    }
    strcpy(buffer + len, chunk);
    len += chunk_len;
  }
  pclose(fp);
  return buffer;
}

// TODO: Use JSON lib
static char *extract_json_string(const char *json, const char *key) {
  if (!json)
    return NULL;
  char search_key[128];
  snprintf(search_key, sizeof(search_key), "\"%s\"", key);
  const char *pos = strstr(json, search_key);
  if (!pos)
    return NULL;

  // Find the colon after the key
  pos = strchr(pos + strlen(search_key), ':');
  if (!pos)
    return NULL;

  // Find the opening quote
  pos = strchr(pos, '"');
  if (!pos)
    return NULL;
  pos++; // Move past the opening quote

  // Find the closing quote
  const char *end = strchr(pos, '"');
  if (!end)
    return NULL;

  size_t len = end - pos;
  char *result = malloc(len + 1);
  if (!result)
    return NULL;
  memcpy(result, pos, len);
  result[len] = '\0';
  return result;
}

static char *extract_broadcast_id(const char *json) {
  if (!json)
    return NULL;
  const char *items_pos = strstr(json, "\"items\"");
  if (!items_pos)
    return NULL;
  return extract_json_string(items_pos, "id");
}

static void oauth_ev_handler(struct mg_connection *c, int ev, void *ev_data) {
  if (ev == MG_EV_HTTP_MSG) {
    struct mg_http_message *hm = (struct mg_http_message *)ev_data;
    struct oauth_ctx *ctx = (struct oauth_ctx *)c->fn_data;

    char code_buf[512] = {0};
    int len = mg_http_get_var(&hm->query, "code", code_buf, sizeof(code_buf));
    if (len > 0) {
      strncpy(ctx->code, code_buf, sizeof(ctx->code) - 1);

      mg_http_reply(c, 200, "Content-Type: text/html\r\n",
                    "<html>"
                    "<head><script>window.close();</script></head>"
                    "<body>Authenticated! You can now close this window.</body>"
                    "</html>");
      ctx->done = 1;
    } else {
      mg_http_reply(c, 400, "Content-Type: text/plain\r\n", "OAuth2 code not found");
    }
  }
}

static void run_oauth_flow(const char *client_id, const char *client_secret, char **out_access_token,
                           char **out_refresh_token) {
  ge_log_info("Triggering Google OAuth loopback flow...");

  char auth_url[2048];
  snprintf(auth_url, sizeof(auth_url),
           "https://accounts.google.com/o/oauth2/v2/auth?"
           "access_type=offline&"
           "scope=https%%3A%%2F%%2Fwww.googleapis.com%%2Fauth%%2Fyoutube.readonly&"
           "include_granted_scopes=true&"
           "response_type=code&"
           "client_id=%s&"
           "redirect_uri=http%%3A%%2F%%2Flocalhost%%3A64119",
           client_id);

  ge_log_info("Opening browser for authorization. URL: %s", auth_url);
  char open_cmd[2048];
  snprintf(open_cmd, sizeof(open_cmd), "open \"%s\"", auth_url);
  system(open_cmd);

  struct mg_mgr mgr;
  struct oauth_ctx ctx = {0};
  mg_mgr_init(&mgr);

  struct mg_connection *c = mg_http_listen(&mgr, "http://0.0.0.0:64119", oauth_ev_handler, &ctx);
  if (!c) {
    ge_log_error("Failed to start temporary OAuth loopback listener on port 64119!");
    mg_mgr_free(&mgr);
    return;
  }

  ge_log_info("Waiting for OAuth code on http://localhost:64119 ...");
  while (!ctx.done) {
    mg_mgr_poll(&mgr, 100);
    usleep(10000); // 10ms
  }

  // Poll a few more times to allow Mongoose to fully flush the HTTP response to the browser socket
  for (int i = 0; i < 5; i++) {
    mg_mgr_poll(&mgr, 100);
  }

  mg_mgr_free(&mgr);
  ge_log_info("OAuth code successfully captured.");

  // Swap code for tokens
  char cmd[4096];
  snprintf(cmd, sizeof(cmd),
           "curl -s -X POST https://oauth2.googleapis.com/token "
           "-d code=%s "
           "-d client_id=%s "
           "-d client_secret=%s "
           "-d redirect_uri=http://localhost:64119 "
           "-d grant_type=authorization_code",
           ctx.code, client_id, client_secret);

  char *response = run_curl(cmd);
  if (response) {
    char *token_path = get_tokens_path();
    if (token_path) {
      write_file(token_path, response);
      bfree(token_path);
    }

    *out_access_token = extract_json_string(response, "access_token");
    *out_refresh_token = extract_json_string(response, "refresh_token");

    free(response);
  }
}

static char *refresh_access_token(const char *client_id, const char *client_secret, const char *refresh_token) {
  ge_log_info("Refreshing expired access token...");

  char cmd[4096];
  snprintf(cmd, sizeof(cmd),
           "curl -s -X POST https://oauth2.googleapis.com/token "
           "-d refresh_token=%s "
           "-d client_id=%s "
           "-d client_secret=%s "
           "-d grant_type=refresh_token",
           refresh_token, client_id, client_secret);

  char *response = run_curl(cmd);
  if (response) {
    char *new_access_token = extract_json_string(response, "access_token");

    if (new_access_token) {
      char merged_json[4096];
      snprintf(merged_json, sizeof(merged_json), "{\"access_token\":\"%s\",\"refresh_token\":\"%s\"}", new_access_token,
               refresh_token);
      char *token_path = get_tokens_path();
      if (token_path) {
        write_file(token_path, merged_json);
        bfree(token_path);
      }
    }

    free(response);
    return new_access_token;
  }
  return NULL;
}

static void *ge_stream_notifier_worker(void *arg) {
  (void)arg;

  ge_log_info("Stream notifier worker thread started.");

  const char *client_id = getenv("GOOGLE_CLIENT_ID");
  const char *client_secret = getenv("GOOGLE_CLIENT_SECRET");
  const char *discord_webhook_url = getenv("DISCORD_WEBHOOK_URL");

  if (!client_id) {
    ge_log_error("GOOGLE_CLIENT_ID is missing or not set in .env!");
    return NULL;
  }
  if (!client_secret) {
    ge_log_error("GOOGLE_CLIENT_SECRET is missing or not set in .env!");
    return NULL;
  }
  if (!discord_webhook_url) {
    ge_log_error("DISCORD_WEBHOOK_URL is missing in .env!");
    return NULL;
  }

  char *access_token = NULL;
  char *refresh_token = NULL;

  char *token_path = get_tokens_path();
  char *tokens_json = token_path ? read_file(token_path) : NULL;
  if (token_path) {
    bfree(token_path);
  }
  if (tokens_json) {
    access_token = extract_json_string(tokens_json, "access_token");
    refresh_token = extract_json_string(tokens_json, "refresh_token");
    free(tokens_json);
  }

  if (!refresh_token) {
    if (access_token) {
      free(access_token);
      access_token = NULL;
    }
    run_oauth_flow(client_id, client_secret, &access_token, &refresh_token);
  }

  if (!access_token || !refresh_token) {
    ge_log_error("Failed to retrieve valid access/refresh tokens.");
    if (access_token)
      free(access_token);
    if (refresh_token)
      free(refresh_token);
    return NULL;
  }

  char cmd[4096];
  snprintf(cmd, sizeof(cmd),
           "curl -s -H \"Authorization: Bearer %s\" "
           "\"https://www.googleapis.com/youtube/v3/liveBroadcasts?part=snippet&mine=true&maxResults=1\"",
           access_token);

  char *response = run_curl(cmd);

  if (!response || strstr(response, "error") || strstr(response, "invalid_grant") || !strstr(response, "items")) {
    ge_log_info("Access token expired or invalid. Attempting refresh...");
    if (response)
      free(response);

    char *new_access = refresh_access_token(client_id, client_secret, refresh_token);
    if (new_access) {
      free(access_token);
      access_token = new_access;

      snprintf(cmd, sizeof(cmd),
               "curl -s -H \"Authorization: Bearer %s\" "
               "\"https://www.googleapis.com/youtube/v3/liveBroadcasts?part=snippet&mine=true&maxResults=1\"",
               access_token);
      response = run_curl(cmd);
    } else {
      ge_log_error("Token refresh failed. Falling back to full OAuth flow...");
      free(access_token);
      free(refresh_token);
      access_token = NULL;
      refresh_token = NULL;
      run_oauth_flow(client_id, client_secret, &access_token, &refresh_token);
      if (access_token) {
        snprintf(cmd, sizeof(cmd),
                 "curl -s -H \"Authorization: Bearer %s\" "
                 "\"https://www.googleapis.com/youtube/v3/liveBroadcasts?part=snippet&mine=true&maxResults=1\"",
                 access_token);
        response = run_curl(cmd);
      }
    }
  }

  if (!response) {
    ge_log_error("YouTube API call returned no response.");
    if (access_token)
      free(access_token);
    if (refresh_token)
      free(refresh_token);
    return NULL;
  }

  char *broadcast_id = extract_broadcast_id(response);
  if (!broadcast_id) {
    ge_log_error("No active live broadcasts found on this YouTube channel.");
    free(response);
    if (access_token)
      free(access_token);
    if (refresh_token)
      free(refresh_token);
    return NULL;
  }

  if (strstr(response, "\"actualEndTime\"")) {
    ge_log_error("Most recent live stream has already ended.");
    free(broadcast_id);
    free(response);
    if (access_token)
      free(access_token);
    if (refresh_token)
      free(refresh_token);
    return NULL;
  }

  char broadcast_url[256];
  snprintf(broadcast_url, sizeof(broadcast_url), "https://youtu.be/%s", broadcast_id);
  ge_log_info("Broadcast found! URL: %s", broadcast_url);

  // Post to Discord
  char discord_cmd[4096];
  snprintf(discord_cmd, sizeof(discord_cmd),
           "curl -s -X POST -H \"Content-Type: application/json\" "
           "-d '{\"content\": \"Now streaming: %s\"}' "
           "\"%s\"",
           broadcast_url, discord_webhook_url);

  char *discord_resp = run_curl(discord_cmd);
  ge_log_info("Successfully sent notification to Discord.");
  if (discord_resp)
    free(discord_resp);

  free(broadcast_id);
  free(response);
  if (access_token)
    free(access_token);
  if (refresh_token)
    free(refresh_token);

  ge_log_info("stream_notifier background task completed successfully.");
  return NULL;
}

void ge_stream_notifier_start(void) {
  pthread_t notifier_thread;
  pthread_attr_t attr;
  pthread_attr_init(&attr);
  pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);

  if (pthread_create(&notifier_thread, &attr, ge_stream_notifier_worker, NULL) != 0) {
    ge_log_error("Failed to spawn stream notifier background thread!");
  }

  pthread_attr_destroy(&attr);
}
