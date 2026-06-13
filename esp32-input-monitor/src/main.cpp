/**
 * Ported to ESP32 from NintendoSpy N64.
 *
 * This is a passive bus sniffer.  It watches the single data wire that runs
 * between an N64 console and a controller and decodes the controller-state
 * packets the controller sends back in response to the console's poll command
 * (0x01).  Nothing is driven onto the line; the ESP32 only reads.
 *
 * The N64 controller protocol encodes each bit as a ~4us pulse on an idle-high,
 * open-collector line: '0' bit -> low for ~3us, high for ~1us, and '1' bit ->
 * low for ~1us, high for ~3us. This firmware uses the ESP32's RMT RX peripheral
 * to timestamp pulse widths in hardware, then decodes each bit from the low
 * pulse duration.
 *
 * Each polled frame on the wire is: the console's 9-bit prefix (the 0x01
 * command byte 0000_0001 followed by a stop bit -> 0000_0001 1) immediately
 * followed by the controller's 32-bit response.
 */

#include <Arduino.h>
#include <ArduinoOTA.h>
#include <ESPAsyncWebServer.h>
#include <ESPmDNS.h>
#include <WiFi.h>
#include <WiFiManager.h>
#include <driver/rmt.h>
#include <freertos/ringbuf.h>

#include "web_ui.h"

// ---------- WiFi / setup portal ---------------------------------------------
// No hardcoded credentials. On first boot (or whenever it can't reconnect) the
// ESP32 brings up an open AP named AP_NAME with a captive portal: join it from
// a phone/laptop, pick your network, and enter the password. WiFiManager saves
// it to flash and the ESP32 reconnects automatically on later boots.
//
// Hold the BOOT button (WIFI_RESET_PIN) for WIFI_RESET_HOLD_MS -- at power-up or
// any time during normal operation -- to forget the saved network and reboot
// into the setup portal.
#define AP_NAME "N64Spy-Setup"
#define MDNS_HOST "n64spy"
#define WIFI_RESET_PIN 0       // BOOT button on most ESP32 dev boards
#define WIFI_RESET_HOLD_MS 3000 // hold this long to wipe WiFi + restart

// ---------- Over-the-air updates --------------------------------------------
// Once connected, the ESP32 listens for firmware uploads over WiFi so you can
// reflash without the USB cable (PlatformIO: `pio run -e esp32dev_ota -t
// upload`). The device advertises itself over mDNS as OTA_HOSTNAME.local; set
// upload_port to that (or its IP) in platformio.ini.
//
// OTA_PASSWORD guards the update endpoint -- leave it empty to disable auth, or
// set one and pass --auth=<password> via upload_flags in platformio.ini.
#define OTA_HOSTNAME MDNS_HOST
#define OTA_PASSWORD ""

// ---------- Wiring -----------------------------------------------------------
// Connect this GPIO to the N64 controller DATA line (the middle pin of the
// 3-pin N64 connector).  The N64 data line is 3.3V logic with a pull-up on the
// console side, so it can be wired directly to an ESP32 input pin -- no level
// shifting required.  Be sure to share a common ground with the
// console/controller.
//
// Up to 4 controller data lines, one per RMT RX channel.
#define N64_CONTROLLER_COUNT 4
#define N64_PIN_1 13
#define N64_PIN_2 12
#define N64_PIN_3 11
#define N64_PIN_4 10
// console command bits that precede the controller's response
#define N64_PREFIX 9
// controller state bits
#define N64_BITCOUNT 32
#define N64_FRAMEBITS (N64_PREFIX + N64_BITCOUNT)

// The console's controller-state poll: command byte 0x01 (0000_0001) followed
// by a stop bit (1).
#define N64_POLL_COMMAND 0x01

#define SERIAL_BAUD 115200

// Cap websocket updates to a UI-friendly rate. Under bursts we keep only the
// newest state and skip intermediate frames rather than flooding AsyncTCP.
#define WS_MIN_SEND_INTERVAL_US 20000
#define WS_MAX_TRACKED_CLIENTS 8
// If a client stays unwritable for this many send attempts, drop it so one
// stuck browser cannot keep accumulating pressure.
#define WS_BLOCKED_STREAK_LIMIT 60

// How long loop() waits (with interrupts ENABLED) for a frame to begin before
// returning. Bounding this is what keeps the interrupt watchdog fed and the
// RTOS scheduled when the line is idle / no console attached.
#define FRAME_WAIT_US 5000

// N64 bit low time: ~1us means logical '1', ~3us means logical '0'.
#define N64_LOW_ONE_MAX_US 2

// Accept only plausible N64 low pulse widths to avoid decoding noise as bits.
#define N64_LOW_MIN_US 1
#define N64_LOW_MAX_US 4

// Each N64 bit cell is about 4us total (low + high). Keep a tolerant range.
#define N64_CELL_MIN_US 3
#define N64_CELL_MAX_US 6

// End an RMT receive once the bus has stayed at one level this long.
#define RMT_IDLE_THRESHOLD_US 12

// Keep enough items for whole transactions plus some jitter/noise margin.
#define RMT_MAX_CAPTURE_BITS 96

// On ESP32-S3 with the legacy RMT API, channels 4-7 are RX-capable.
static constexpr int kN64Pins[N64_CONTROLLER_COUNT] = {
  N64_PIN_1, N64_PIN_2, N64_PIN_3, N64_PIN_4};
static constexpr rmt_channel_t kN64Channels[N64_CONTROLLER_COUNT] = {
  RMT_CHANNEL_4, RMT_CHANNEL_5, RMT_CHANNEL_6, RMT_CHANNEL_7};
static RingbufHandle_t n64RmtRingbufs[N64_CONTROLLER_COUNT] = {nullptr, nullptr,
                                 nullptr, nullptr};

// HTTP server (serves the UI) and the WebSocket the UI listens on. Both run in
// the AsyncTCP task on the other core, so they never block pulse capture.
static AsyncWebServer server(80);
static AsyncWebSocket ws("/ws");

// Last broadcast state per controller, so a client connecting mid-session gets
// current values without waiting for the next button change.
static uint8_t lastPayload[N64_CONTROLLER_COUNT][4] = {
  {0, 0, 0, 0}, {0, 0, 0, 0}, {0, 0, 0, 0}, {0, 0, 0, 0}};
// Pending websocket packet format: [controllerIndex, 4-byte state payload].
static uint8_t pendingPacket[N64_CONTROLLER_COUNT][5] = {};
static bool hasPendingPacket[N64_CONTROLLER_COUNT] = {false, false, false, false};
static uint32_t lastWsSendAtUs = 0;
static uint32_t wsDiscardCount = 0;
static uint32_t wsDisconnectCount = 0;
static uint32_t wsSlowCloseCount = 0;
static uint32_t wifiDisconnectCount = 0;
static uint32_t lastWsDiagAtMs = 0;
static uint8_t nextPendingController = 0;

// A port is considered "connected" while valid poll-response frames are seen
// recently. This is activity-based detection (not direct cable detection).
#define PORT_ACTIVITY_TIMEOUT_MS 1500
#define PORT_PROBE_INTERVAL_MS 100
#define PORT_PROBE_WINDOW_MS 30
static bool controllerConnected[N64_CONTROLLER_COUNT] = {false, false, false,
                                                         false};
static uint32_t controllerLastSeenMs[N64_CONTROLLER_COUNT] = {0, 0, 0, 0};
static bool controllerRxRunning[N64_CONTROLLER_COUNT] = {false, false, false,
                                                         false};
static bool controllerProbing[N64_CONTROLLER_COUNT] = {false, false, false,
                                                       false};
static uint32_t controllerProbeStartedMs[N64_CONTROLLER_COUNT] = {0, 0, 0, 0};
static uint32_t controllerLastProbeAtMs[N64_CONTROLLER_COUNT] = {0, 0, 0, 0};

struct TrackedWsClient {
  uint32_t id;
  uint16_t blockedStreak;
  bool active;
};

static TrackedWsClient trackedWsClients[WS_MAX_TRACKED_CLIENTS] = {};

static inline bool isPollResponse(const uint8_t *frame);

static TrackedWsClient *findTrackedClient(uint32_t id) {
  for (size_t i = 0; i < WS_MAX_TRACKED_CLIENTS; ++i) {
    if (trackedWsClients[i].active && trackedWsClients[i].id == id) {
      return &trackedWsClients[i];
    }
  }
  return nullptr;
}

static TrackedWsClient *upsertTrackedClient(uint32_t id) {
  TrackedWsClient *slot = findTrackedClient(id);
  if (slot != nullptr) {
    return slot;
  }

  for (size_t i = 0; i < WS_MAX_TRACKED_CLIENTS; ++i) {
    if (!trackedWsClients[i].active) {
      trackedWsClients[i].active = true;
      trackedWsClients[i].id = id;
      trackedWsClients[i].blockedStreak = 0;
      return &trackedWsClients[i];
    }
  }

  return nullptr;
}

static void removeTrackedClient(uint32_t id) {
  TrackedWsClient *slot = findTrackedClient(id);
  if (slot == nullptr) {
    return;
  }
  slot->active = false;
  slot->id = 0;
  slot->blockedStreak = 0;
}

/** Configure one controller's RMT RX channel for 1us pulse capture. */
static bool startRmtCapture(size_t controller) {
  if (controller >= N64_CONTROLLER_COUNT) {
    return false;
  }

  rmt_config_t cfg = {};
  cfg.rmt_mode = RMT_MODE_RX;
  cfg.channel = kN64Channels[controller];
  cfg.gpio_num = (gpio_num_t)kN64Pins[controller];
  cfg.clk_div = 80; // 80MHz APB / 80 = 1MHz tick => 1us resolution.
  // ESP32-S3 legacy RMT has tight per-group memory; with 4 RX channels active,
  // use one block per channel so all channels can be configured.
  cfg.mem_block_num = 1;
  cfg.flags = 0;
  cfg.rx_config.idle_threshold = RMT_IDLE_THRESHOLD_US;
  // Drop sub-microsecond glitches before they hit the RX ring buffer.
  cfg.rx_config.filter_en = true;
  cfg.rx_config.filter_ticks_thresh = 1;

  esp_err_t err = rmt_config(&cfg);
  if (err != ESP_OK) {
    Serial.printf("RMT config failed (pad %u): %d\n", (unsigned)(controller + 1),
                  (int)err);
    return false;
  }

  err = rmt_driver_install(kN64Channels[controller], 4096, 0);
  if (err != ESP_OK) {
    Serial.printf("RMT driver install failed (pad %u): %d\n",
                  (unsigned)(controller + 1), (int)err);
    return false;
  }

  err = rmt_get_ringbuf_handle(kN64Channels[controller],
                               &n64RmtRingbufs[controller]);
  if (err != ESP_OK || n64RmtRingbufs[controller] == nullptr) {
    Serial.printf("RMT ringbuf setup failed (pad %u): %d\n",
                  (unsigned)(controller + 1), (int)err);
    return false;
  }

  controllerRxRunning[controller] = false;
  controllerProbing[controller] = false;
  return true;
}

static bool startControllerRx(size_t controller) {
  if (controller >= N64_CONTROLLER_COUNT) {
    return false;
  }
  if (controllerRxRunning[controller]) {
    return true;
  }

  esp_err_t err = rmt_rx_start(kN64Channels[controller], true);
  if (err != ESP_OK) {
    Serial.printf("RMT RX start failed (pad %u): %d\n",
                  (unsigned)(controller + 1),
                  (int)err);
    return false;
  }

  controllerRxRunning[controller] = true;
  return true;
}

static void stopControllerRx(size_t controller) {
  if (controller >= N64_CONTROLLER_COUNT || !controllerRxRunning[controller]) {
    return;
  }

  esp_err_t err = rmt_rx_stop(kN64Channels[controller]);
  if (err != ESP_OK) {
    Serial.printf("RMT RX stop failed (pad %u): %d\n", (unsigned)(controller + 1),
                  (int)err);
  }
  controllerRxRunning[controller] = false;
}

/** Convert a low pulse width in microseconds to an N64 bit value. */
static inline uint8_t decodeBitFromLowUs(uint32_t lowUs) {
  return (lowUs <= N64_LOW_ONE_MAX_US) ? 1U : 0U;
}

/** Validate that one low/high pulse pair looks like a real N64 bit cell. */
static inline bool isValidN64CellUs(uint32_t lowUs, uint32_t highUs) {
  if (lowUs < N64_LOW_MIN_US || lowUs > N64_LOW_MAX_US) {
    return false;
  }

  uint32_t total = lowUs + highUs;
  return total >= N64_CELL_MIN_US && total <= N64_CELL_MAX_US;
}

/**
 * Decode an RMT packet into an N64 frame (9-bit poll prefix + 32-bit response).
 * Returns true if a full poll-response frame is found.
 */
static bool decodeFrameFromRmtItems(const rmt_item32_t *items, size_t count,
                                    uint8_t frame[N64_FRAMEBITS]) {
  uint8_t bits[RMT_MAX_CAPTURE_BITS];
  size_t bitCount = 0;

  for (size_t i = 0; i < count && bitCount < RMT_MAX_CAPTURE_BITS; ++i) {
    const rmt_item32_t &item = items[i];

    // Valid N64 traffic is low->high for each bit cell. Decode only those
    // cells and ignore malformed/noisy segments.
    if (item.level0 == 0 && item.level1 == 1 && item.duration0 > 0 &&
        item.duration1 > 0 &&
        isValidN64CellUs(item.duration0, item.duration1)) {
      bits[bitCount++] = decodeBitFromLowUs(item.duration0);
    }

    if (item.level0 == 1 && item.level1 == 0 && item.duration0 > 0 &&
        item.duration1 > 0 && bitCount < RMT_MAX_CAPTURE_BITS &&
        isValidN64CellUs(item.duration1, item.duration0)) {
      bits[bitCount++] = decodeBitFromLowUs(item.duration1);
    }
  }

  if (bitCount != N64_FRAMEBITS) {
    return false;
  }

  if (!isPollResponse(bits)) {
    return false;
  }

  memcpy(frame, bits, N64_FRAMEBITS);
  return true;
}

/** Poll one controller's RMT ring buffer and decode one N64 frame if present. */
static bool readFrameFromRmt(size_t controller, uint8_t frame[N64_FRAMEBITS]) {
  if (controller >= N64_CONTROLLER_COUNT || n64RmtRingbufs[controller] == nullptr) {
    return false;
  }

  bool foundFrame = false;

  for (;;) {
    size_t rxSize = 0;
    rmt_item32_t *items = (rmt_item32_t *)xRingbufferReceive(
        n64RmtRingbufs[controller], &rxSize, 0);
    if (items == nullptr) {
      break;
    }

    if (rxSize >= sizeof(rmt_item32_t)) {
      uint8_t latestFrame[N64_FRAMEBITS];
      if (decodeFrameFromRmtItems(items, rxSize / sizeof(rmt_item32_t),
                                  latestFrame)) {
        memcpy(frame, latestFrame, N64_FRAMEBITS);
        foundFrame = true;
      }
    }

    vRingbufferReturnItem(n64RmtRingbufs[controller], (void *)items);
  }

  return foundFrame;
}

/** Decode one MSB-first byte from the 8 bits of `bits` starting at `offset`. */
static inline uint8_t readByte(const uint8_t *bits, int offset) {
  uint8_t val = 0;
  for (int i = 0; i < 8; ++i) {
    if (bits[offset + i]) {
      val |= (uint8_t)(1 << (7 - i));
    }
  }
  return val;
}

/** True if the 9-bit prefix is the console's poll command (byte 0x01,
 * MSB-first) followed by a stop bit (1). */
static inline bool isPollResponse(const uint8_t *frame) {
  const uint8_t command = readByte(frame, 0); // first 8 prefix bits
  const uint8_t stopBit = frame[8];           // 9th prefix bit
  return command == N64_POLL_COMMAND && stopBit == 1;
}

/**
 * Bits 8 and 9 in the 32-bit controller response are unused and expected to be
 * zero on valid packets. This rejects many random/noisy false decodes.
 */
static inline bool hasValidReservedBits(const uint8_t *frame) {
  const uint8_t *r = frame + N64_PREFIX;
  return r[8] == 0 && r[9] == 0;
}

/** Decoded N64 controller state. */
struct N64State {
  bool a, b, z, start;
  bool up, down, left, right;
  bool l, r;
  bool cUp, cDown, cLeft, cRight;
  int8_t stickX;
  int8_t stickY;
};

/**
 * Bit positions within the 32-bit controller response, matching NintendoSpy's
 * Readers/Nintendo64.cs. (Indices 8 and 9 are unused by the controller.)
 */
enum N64ResponseBit {
  RESP_A = 0,
  RESP_B,
  RESP_Z,
  RESP_START,
  RESP_UP,
  RESP_DOWN,
  RESP_LEFT,
  RESP_RIGHT,
  RESP_L = 10,
  RESP_R,
  RESP_C_UP,
  RESP_C_DOWN,
  RESP_C_LEFT,
  RESP_C_RIGHT,
  RESP_STICK_X = 16, // 8-bit signed, MSB-first
  RESP_STICK_Y = 24, // 8-bit signed, MSB-first
};

/** Decode the controller response that follows the prefix in `frame`. */
static N64State decodeState(const uint8_t *frame) {
  const uint8_t *r = frame + N64_PREFIX; // start of the 32-bit response
  N64State s;
  s.a = r[RESP_A];
  s.b = r[RESP_B];
  s.z = r[RESP_Z];
  s.start = r[RESP_START];
  s.up = r[RESP_UP];
  s.down = r[RESP_DOWN];
  s.left = r[RESP_LEFT];
  s.right = r[RESP_RIGHT];
  s.l = r[RESP_L];
  s.r = r[RESP_R];
  s.cUp = r[RESP_C_UP];
  s.cDown = r[RESP_C_DOWN];
  s.cLeft = r[RESP_C_LEFT];
  s.cRight = r[RESP_C_RIGHT];
  s.stickX = (int8_t)readByte(r, RESP_STICK_X);
  s.stickY = (int8_t)readByte(r, RESP_STICK_Y);
  return s;
}

/** Pretty-print one controller's state over serial. */
static void printState(size_t controller, const N64State &s) {
  char buf[160];
  int n = 0;
  n += snprintf(buf + n, sizeof(buf) - n, "[N64 %u]",
                (unsigned)(controller + 1));

#define BTN(label, field)                                                      \
  do {                                                                         \
    if (s.field)                                                               \
      n += snprintf(buf + n, sizeof(buf) - n, " " label);                      \
  } while (0)
  BTN("A", a);
  BTN("B", b);
  BTN("Z", z);
  BTN("START", start);
  BTN("UP", up);
  BTN("DOWN", down);
  BTN("LEFT", left);
  BTN("RIGHT", right);
  BTN("L", l);
  BTN("R", r);
  BTN("C-UP", cUp);
  BTN("C-DOWN", cDown);
  BTN("C-LEFT", cLeft);
  BTN("C-RIGHT", cRight);
#undef BTN

  n += snprintf(buf + n, sizeof(buf) - n, "  stick=(%d, %d)", s.stickX,
                s.stickY);
  Serial.println(buf);
}

/**
 * Pack the 32-bit controller response into 4 bytes for the wire. Each button
 * byte is MSB-first (matching readByte); the layout is mirrored by the bit
 * masks in web_ui.h:
 *   [0] A B Z START UP DOWN LEFT RIGHT
 *   [1] - - L R C-UP C-DOWN C-LEFT C-RIGHT   (top 2 bits are the unused 8,9)
 *   [2] stick X (int8)   [3] stick Y (int8)
 */
static void packState(const uint8_t *frame, uint8_t out[4]) {
  const uint8_t *r = frame + N64_PREFIX; // start of the 32-bit response
  out[0] = readByte(r, 0);
  out[1] = readByte(r, 8);
  out[2] = readByte(r, 16);
  out[3] = readByte(r, 24);
}

static bool anyPendingPackets() {
  for (size_t i = 0; i < N64_CONTROLLER_COUNT; ++i) {
    if (hasPendingPacket[i]) {
      return true;
    }
  }
  return false;
}

/**
 * Push the newest pending state at a bounded rate. Under bursts we keep only
 * the latest unsent state instead of flooding websocket queues with stale
 * intermediate frames.
 */
static void flushPendingPayload() {
  if (!anyPendingPackets()) {
    return;
  }

  uint32_t nowUs = micros();
  if ((uint32_t)(nowUs - lastWsSendAtUs) < WS_MIN_SEND_INTERVAL_US) {
    return;
  }

  bool hasLiveClient = false;

  int controllerToSend = -1;
  for (size_t i = 0; i < N64_CONTROLLER_COUNT; ++i) {
    size_t idx = (nextPendingController + i) % N64_CONTROLLER_COUNT;
    if (hasPendingPacket[idx]) {
      controllerToSend = (int)idx;
      break;
    }
  }

  if (controllerToSend < 0) {
    return;
  }

  bool sentAny = false;
  const uint8_t *packet = pendingPacket[controllerToSend];

  for (size_t i = 0; i < WS_MAX_TRACKED_CLIENTS; ++i) {
    TrackedWsClient &tracked = trackedWsClients[i];
    if (!tracked.active) {
      continue;
    }

    if (!ws.hasClient(tracked.id)) {
      tracked.active = false;
      tracked.id = 0;
      tracked.blockedStreak = 0;
      continue;
    }

    hasLiveClient = true;

    if (!ws.availableForWrite(tracked.id)) {
      ++tracked.blockedStreak;
      if (tracked.blockedStreak >= WS_BLOCKED_STREAK_LIMIT) {
        Serial.printf("[ws] closing slow client id=%lu\n",
                      (unsigned long)tracked.id);
        ws.close(tracked.id, 1013, "server busy");
        ++wsSlowCloseCount;
        tracked.active = false;
        tracked.id = 0;
        tracked.blockedStreak = 0;
      }
      continue;
    }

    tracked.blockedStreak = 0;
    if (ws.binary(tracked.id, packet, sizeof(pendingPacket[controllerToSend]))) {
      sentAny = true;
    } else {
      ++wsDiscardCount;
    }
  }

  if (!hasLiveClient) {
    for (size_t i = 0; i < N64_CONTROLLER_COUNT; ++i) {
      hasPendingPacket[i] = false;
    }
    return;
  }

  if (sentAny) {
    hasPendingPacket[controllerToSend] = false;
    nextPendingController =
        (uint8_t)((controllerToSend + 1) % N64_CONTROLLER_COUNT);
    lastWsSendAtUs = nowUs;
  }
}

/** Periodically summarize websocket/Wi-Fi health without spamming serial. */
static void logWsDiagnostics() {
  uint32_t nowMs = millis();
  if (nowMs - lastWsDiagAtMs < 1000) {
    return;
  }

  lastWsDiagAtMs = nowMs;
  if (wsDiscardCount == 0 && wsDisconnectCount == 0 && wsSlowCloseCount == 0 &&
      wifiDisconnectCount == 0) {
    return;
  }

  Serial.printf("[ws] clients=%u pending=%u discards=%lu ws_disc=%lu slow_close=%lu wifi_disc=%lu\n",
                (unsigned)ws.count(), anyPendingPackets() ? 1U : 0U,
                (unsigned long)wsDiscardCount,
                (unsigned long)wsDisconnectCount,
                (unsigned long)wsSlowCloseCount,
                (unsigned long)wifiDisconnectCount);
  wsDiscardCount = 0;
  wsDisconnectCount = 0;
  wsSlowCloseCount = 0;
  wifiDisconnectCount = 0;
}

/** Update per-port connected/disconnected state from recent frame activity. */
static void refreshControllerConnectionState() {
  uint32_t nowMs = millis();

  for (size_t controller = 0; controller < N64_CONTROLLER_COUNT; ++controller) {
    if (!controllerConnected[controller]) {
      continue;
    }

    if ((uint32_t)(nowMs - controllerLastSeenMs[controller]) <=
        PORT_ACTIVITY_TIMEOUT_MS) {
      continue;
    }

    controllerConnected[controller] = false;
    controllerProbing[controller] = false;
    stopControllerRx(controller);
    Serial.printf("[port %u] disconnected (no poll traffic)\n",
                  (unsigned)(controller + 1));
  }
}

/**
 * Auto-probe disconnected ports: keep RX stopped most of the time, then open
 * short probe windows to discover newly active controller lines.
 */
static void serviceControllerProbing() {
  uint32_t nowMs = millis();

  for (size_t controller = 0; controller < N64_CONTROLLER_COUNT; ++controller) {
    if (controllerConnected[controller]) {
      if (!controllerRxRunning[controller]) {
        startControllerRx(controller);
      }
      continue;
    }

    if (!controllerRxRunning[controller]) {
      if ((uint32_t)(nowMs - controllerLastProbeAtMs[controller]) <
          PORT_PROBE_INTERVAL_MS) {
        continue;
      }

      if (startControllerRx(controller)) {
        controllerProbing[controller] = true;
        controllerProbeStartedMs[controller] = nowMs;
        controllerLastProbeAtMs[controller] = nowMs;
      }
      continue;
    }

    if (controllerProbing[controller] &&
        (uint32_t)(nowMs - controllerProbeStartedMs[controller]) >=
            PORT_PROBE_WINDOW_MS) {
      controllerProbing[controller] = false;
      stopControllerRx(controller);
    }
  }
}

/** Send the current state to a client as soon as it connects. */
static void onWsEvent(AsyncWebSocket *server, AsyncWebSocketClient *client,
                      AwsEventType type, void *arg, uint8_t *data, size_t len) {
  if (type == WS_EVT_CONNECT) {
    client->setCloseClientOnQueueFull(false);
    if (upsertTrackedClient(client->id()) == nullptr) {
      Serial.printf("[ws] too many tracked clients; closing id=%lu\n",
                    (unsigned long)client->id());
      ws.close(client->id(), 1008, "too many clients");
      return;
    }
    Serial.printf("[ws] connect id=%lu from=%s\n", (unsigned long)client->id(),
                  client->remoteIP().toString().c_str());
    for (size_t i = 0; i < N64_CONTROLLER_COUNT; ++i) {
      uint8_t packet[5];
      packet[0] = (uint8_t)i;
      memcpy(packet + 1, lastPayload[i], sizeof(lastPayload[i]));
      client->binary(packet, sizeof(packet));
    }
  } else if (type == WS_EVT_DISCONNECT) {
    removeTrackedClient(client->id());
    ++wsDisconnectCount;
    Serial.printf("[ws] disconnect id=%lu from=%s\n",
                  (unsigned long)client->id(),
                  client->remoteIP().toString().c_str());
  }
}

/** Log link-level Wi-Fi transitions so browser drops can be correlated. */
static void onWiFiEvent(WiFiEvent_t event, arduino_event_info_t info) {
  if (event == ARDUINO_EVENT_WIFI_STA_GOT_IP) {
    Serial.printf("[wifi] got ip=%s\n", WiFi.localIP().toString().c_str());
  } else if (event == ARDUINO_EVENT_WIFI_STA_DISCONNECTED) {
    ++wifiDisconnectCount;
    Serial.printf("[wifi] disconnected reason=%d\n",
                  info.wifi_sta_disconnected.reason);
  }
}

/**
 * Start the ArduinoOTA listener so the firmware can be reflashed over WiFi.
 * mDNS is already up by the time this is called, so OTA just adds its service
 * to the existing responder. The handlers below only log progress -- the actual
 * write/reboot is handled by the library.
 */
static void startOTA() {
  ArduinoOTA.setHostname(OTA_HOSTNAME);
  if (strlen(OTA_PASSWORD) > 0) {
    ArduinoOTA.setPassword(OTA_PASSWORD);
  }

  ArduinoOTA.onStart([]() {
    Serial.println("OTA update starting -- pausing controller sniffing.");
  });
  ArduinoOTA.onEnd([]() { Serial.println("\nOTA update complete; rebooting."); });
  ArduinoOTA.onProgress([](unsigned int done, unsigned int total) {
    Serial.printf("OTA progress: %u%%\r", (done * 100) / total);
  });
  ArduinoOTA.onError([](ota_error_t error) {
    Serial.printf("OTA error [%u]\n", error);
  });

  ArduinoOTA.begin();
  Serial.printf("OTA ready -- flash to %s.local\n", OTA_HOSTNAME);
}

/** Erase the saved WiFi network and reboot (into the setup portal). */
static void clearWiFiAndRestart() {
  Serial.println("Erasing saved WiFi and restarting into setup portal...");
  WiFiManager wm;
  wm.resetSettings();
  delay(200); // let the serial line flush
  ESP.restart();
}

/**
 * Bring up WiFi via the captive setup portal and start mDNS + the web server.
 * Blocks in setup() until connected (or restarts on portal timeout), before the
 * async server starts -- so WiFiManager's own server never clashes with ours.
 */
static void startNetwork() {
  WiFi.mode(WIFI_STA);
  // Disable modem sleep: lower latency/jitter and fewer websocket transport
  // interruptions under sustained traffic.
  WiFi.setSleep(false);
  WiFi.onEvent(onWiFiEvent);

  WiFiManager wm;
  pinMode(WIFI_RESET_PIN, INPUT_PULLUP);
  if (digitalRead(WIFI_RESET_PIN) == LOW) {
    Serial.println("BOOT held -- forgetting saved WiFi, opening setup portal.");
    wm.resetSettings();
  }

  // The captive portal runs its own (sync) WebServer on port 80, which doesn't
  // release the socket in time for our AsyncWebServer to bind on the same boot.
  // So if the portal had to run, reboot once it has saved creds: the next boot
  // connects directly without the portal, leaving port 80 free for us.
  bool justConfigured = false;
  wm.setSaveConfigCallback([&]() { justConfigured = true; });

  // Try saved creds, else open the captive portal to collect new ones. On
  // timeout we restart rather than hang forever, so a brief router outage just
  // retries on the next boot.
  wm.setConfigPortalTimeout(180);
  Serial.printf("Joining WiFi (or open the \"%s\" network to configure)...\n",
                AP_NAME);
  if (!wm.autoConnect(AP_NAME)) {
    Serial.println("WiFi setup timed out; restarting.");
    delay(1000);
    ESP.restart();
  }

  if (justConfigured) {
    Serial.println("WiFi saved -- rebooting to start the web server cleanly.");
    delay(500);
    ESP.restart();
  }

  Serial.printf("Connected. Open http://%s/", WiFi.localIP().toString().c_str());
  if (MDNS.begin(MDNS_HOST)) {
    MDNS.addService("http", "tcp", 80);
    Serial.printf(" or http://%s.local/", MDNS_HOST);
  }
  Serial.println();

  ws.onEvent(onWsEvent);
  server.addHandler(&ws);
  // Qualify the enum: WiFiManager pulls in the WebServer library, which also
  // defines an HTTP_GET, so the bare name is ambiguous here.
  server.on("/", WebRequestMethod::HTTP_GET, [](AsyncWebServerRequest *req) {
    req->send(200, "text/html", INDEX_HTML);
  });
  server.begin();

  startOTA();
}

/** One-time init: serial, the input pin, and a startup banner. */
void setup() {
  Serial.begin(SERIAL_BAUD);

  // The N64 line has a pull-up on the console side. Enabling the (weak)
  // internal pull-up too means disconnected pins read idle-high instead of
  // floating, so we see clean "no activity" rather than noise.
  for (size_t i = 0; i < N64_CONTROLLER_COUNT; ++i) {
    pinMode(kN64Pins[i], INPUT_PULLUP);
  }

  delay(50);
  Serial.println();

  Serial.printf("NintendoSpy N64 reader (ESP32) ready @ %lu MHz\n",
                (unsigned long)(F_CPU / 1000000UL));

  for (size_t i = 0; i < N64_CONTROLLER_COUNT; ++i) {
    Serial.printf("pad %u: GPIO %d via RMT channel %d\n", (unsigned)(i + 1),
                  kN64Pins[i], (int)kN64Channels[i]);
    if (!startRmtCapture(i)) {
      Serial.printf("RMT capture init failed for pad %u; restarting in 2s...\n",
                    (unsigned)(i + 1));
      delay(2000);
      ESP.restart();
    }
  }

  Serial.println(
      "Sniffing up to 4 N64 controller data lines... press buttons to see input.");

  startNetwork();
}

/**
 * Wipe WiFi + reboot if the BOOT button is held for WIFI_RESET_HOLD_MS. Polled
 * every loop iteration (even when the console is idle), so the hold is timed
 * across calls with millis() rather than blocking here.
 */
static void checkResetButton() {
  static uint32_t pressedAt = 0;
  if (digitalRead(WIFI_RESET_PIN) == LOW) {
    if (pressedAt == 0) {
      pressedAt = millis();
    } else if (millis() - pressedAt >= WIFI_RESET_HOLD_MS) {
      clearWiFiAndRestart();
    }
  } else {
    pressedAt = 0; // released before the hold completed -- reset the timer
  }
}

/** Sniff one frame off the wire, decode it, and log the state when it changes.
 */
void loop() {
  checkResetButton();
  // Service any in-flight OTA upload. Cheap when idle; blocks here for the few
  // seconds of an actual flash (sniffing pauses, then the device reboots).
  ArduinoOTA.handle();
  flushPendingPayload();
  logWsDiagnostics();
  refreshControllerConnectionState();
  serviceControllerProbing();

#ifdef DEBUG_HEARTBEAT
  // Build with `-D DEBUG_HEARTBEAT` to confirm the loop is alive even when no
  // console is polling the line (otherwise serial is silent until a button
  // changes). Throttled so it doesn't drown out real output.
  static uint32_t lastBeat = 0;
  if (millis() - lastBeat > 2000) {
    Serial.println("[idle] loop alive, waiting for N64 poll...");
    lastBeat = millis();
  }
#endif

  // Reap any disconnected WebSocket clients (throttled; cheap when idle).
  static uint32_t lastCleanup = 0;
  if (millis() - lastCleanup > 1000) {
    ws.cleanupClients();
    lastCleanup = millis();
  }

  for (size_t controller = 0; controller < N64_CONTROLLER_COUNT; ++controller) {
    uint8_t frame[N64_FRAMEBITS];
    if (!readFrameFromRmt(controller, frame)) {
      continue;
    }
    if (!hasValidReservedBits(frame)) {
      continue;
    }

    uint8_t payload[4];
    packState(frame, payload);

    // Broadcast (and log) only on change per controller.
    if (memcmp(lastPayload[controller], payload, sizeof(payload)) != 0) {
      memcpy(lastPayload[controller], payload, sizeof(payload));
      pendingPacket[controller][0] = (uint8_t)controller;
      memcpy(pendingPacket[controller] + 1, payload, sizeof(payload));
      hasPendingPacket[controller] = true;
      printState(controller, decodeState(frame));
    }

    uint32_t nowMs = millis();
    controllerLastSeenMs[controller] = nowMs;
    if (!controllerConnected[controller]) {
      controllerConnected[controller] = true;
      controllerProbing[controller] = false;
      Serial.printf("[port %u] connected (poll traffic detected)\n",
                    (unsigned)(controller + 1));
    }
  }

  flushPendingPayload();
}
