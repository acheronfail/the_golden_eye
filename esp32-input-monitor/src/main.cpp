/**
 * Ported to ESP32 from NintendoSpy N64.
 *
 * This is a passive bus sniffer.  It watches the single data wire that runs
 * between an N64 console and a controller and decodes the controller-state
 * packets the controller sends back in response to the console's poll command
 * (0x01).  Nothing is driven onto the line; the ESP32 only reads.
 *
 * The N64 controller protocol encodes each bit as a ~4us pulse on an idle-high,
 * open-collector line: '0' bit -> low for ~3us, high for ~1us '1' bit -> low
 * for ~1us, high for ~3us So, after every falling edge, sampling the line ~2us
 * later yields the bit's value: a '1' has already returned high while a '0' is
 * still held low.  This is the same trick the original AVR firmware uses; only
 * the timing primitives (cycle counter instead of hand-counted NOPs) and the
 * GPIO access (register read instead of PIND) differ.
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
// Pick an input-capable GPIO below 32 (the fast read path below assumes
// GPIO.in, which covers pins 0-31).
#define N64_PIN 4
// console command bits that precede the controller's response
#define N64_PREFIX 9
// controller state bits
#define N64_BITCOUNT 32
#define N64_FRAMEBITS (N64_PREFIX + N64_BITCOUNT)

// The console's controller-state poll: command byte 0x01 (0000_0001) followed
// by a stop bit (1).
#define N64_POLL_COMMAND 0x01

#define SERIAL_BAUD 115200

// Sample the line this many microseconds after each falling edge (middle of the
// bit's decisive window).
#define SAMPLE_DELAY_US 2

// Once a frame is underway, give up on the next bit's falling edge after this
// long (a valid bit arrives every ~4us). This bounds how long interrupts stay
// disabled, so a truncated/glitchy frame can't hang us.
#define BIT_TIMEOUT_US 60

// How long loop() waits (with interrupts ENABLED) for a frame to begin before
// returning. Bounding this is what keeps the interrupt watchdog fed and the
// RTOS scheduled when the line is idle / no console attached.
#define FRAME_WAIT_US 5000

// Cycle-counter helpers: at the default 240MHz, 1us == 240 CPU cycles.  F_CPU
// gives the real value.
#define CYCLES_PER_US (F_CPU / 1000000UL)

// Fast, single-instruction read of the data line.  Valid for GPIO 0-31.
#define READ_PIN() ((GPIO.in >> N64_PIN) & 0x1U)

// Critical section guard so the bit-banged read is not preempted mid-frame.
static portMUX_TYPE n64Mux = portMUX_INITIALIZER_UNLOCKED;

// HTTP server (serves the UI) and the WebSocket the UI listens on. Both run in
// the AsyncTCP task on the other core, so they never disturb the bit-bang read.
static AsyncWebServer server(80);
static AsyncWebSocket ws("/ws");

// Last broadcast state, so a client connecting mid-session gets the current
// state immediately instead of waiting for the next button change.
static uint8_t lastPayload[4] = {0, 0, 0, 0};

/** Busy-wait for an exact number of CPU cycles using the Xtensa cycle counter.
 */
static inline IRAM_ATTR void waitCycles(uint32_t cycles) {
  uint32_t start = xthal_get_ccount();
  while (xthal_get_ccount() - start < cycles) { /* spin */
  }
}

/**
 * Wait for the data line to be idle-high and then fall (a falling edge), giving
 * up after `timeoutCycles`. Returns true if the edge was seen, false on
 * timeout. Used both to detect the start of a frame (interrupts on) and to step
 * to each subsequent bit (interrupts off) -- the timeout is what keeps either
 * case bounded.
 */
static inline IRAM_ATTR bool waitFallingEdge(uint32_t timeoutCycles) {
  uint32_t start = xthal_get_ccount();
  while (!READ_PIN()) {
    if (xthal_get_ccount() - start > timeoutCycles) {
      return false;
    }
  } // wait for line high
  while (READ_PIN()) {
    if (xthal_get_ccount() - start > timeoutCycles) {
      return false;
    }
  } // wait for line low
  return true;
}

/**
 * Read `bits` bits off the one-wire line into `buffer`, one byte (0/1) per
 * entry. The caller must have already detected the falling edge of bit 0 (see
 * loop()). Returns false if the frame ends early (line goes idle).
 */
static IRAM_ATTR bool readOneWire(uint8_t *buffer, uint8_t bits) {
  // Bit 0: its falling edge was already detected by the frame-start wait in
  // loop().
  waitCycles(SAMPLE_DELAY_US * CYCLES_PER_US);
  buffer[0] = READ_PIN();

  for (uint8_t i = 1; i < bits; ++i) {
    if (!waitFallingEdge(BIT_TIMEOUT_US * CYCLES_PER_US)) {
      return false;
    }
    waitCycles(SAMPLE_DELAY_US * CYCLES_PER_US);
    buffer[i] = READ_PIN();
  }
  return true;
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

/** Pretty-print the state over serial. */
static void printState(const N64State &s) {
  char buf[160];
  int n = 0;
  n += snprintf(buf + n, sizeof(buf) - n, "[N64]");

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

/** Send the current state to a client as soon as it connects. */
static void onWsEvent(AsyncWebSocket *server, AsyncWebSocketClient *client,
                      AwsEventType type, void *arg, uint8_t *data, size_t len) {
  if (type == WS_EVT_CONNECT) {
    client->binary(lastPayload, sizeof(lastPayload));
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
  // internal pull-up too means a disconnected pin reads idle-high instead of
  // floating, so we see clean "no activity" rather than noise.
  pinMode(N64_PIN, INPUT_PULLUP);
  delay(50);
  Serial.println();
  Serial.printf("NintendoSpy N64 reader (ESP32) ready on GPIO %d @ %lu MHz\n",
                N64_PIN, (unsigned long)(F_CPU / 1000000UL));
  Serial.println(
      "Sniffing N64 controller data line... press buttons to see input.");

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

  // Wait for a frame to start with interrupts ENABLED and a timeout: the
  // idle-high line falling marks the first bit. Returning on timeout lets the
  // RTOS run and feeds the watchdogs when the line is idle.
  if (!waitFallingEdge(FRAME_WAIT_US * CYCLES_PER_US)) {
    return;
  }

  // The bits read off the wire this frame (prefix + response), one 0/1 byte
  // each.
  uint8_t frame[N64_FRAMEBITS];

  // A frame is underway -- read the rest with interrupts off so the bit timing
  // isn't disturbed. The per-bit timeout inside readOneWire bounds how long
  // interrupts stay disabled (~a couple of ms worst case).
  portENTER_CRITICAL(&n64Mux);
  bool complete = readOneWire(frame, N64_FRAMEBITS);
  portEXIT_CRITICAL(&n64Mux);

  if (!complete || !isPollResponse(frame)) {
    // Truncated frame, or not a controller-state poll (e.g. a rumble/mempak
    // command). Ignore it.
    return;
  }

  // Reap any disconnected WebSocket clients (throttled; cheap when idle).
  static uint32_t lastCleanup = 0;
  if (millis() - lastCleanup > 1000) {
    ws.cleanupClients();
    lastCleanup = millis();
  }

  uint8_t payload[4];
  packState(frame, payload);

  // Broadcast (and log) only on change -- WebSocket is ordered/reliable, so a
  // new client gets the current state via lastPayload on connect.
  if (memcmp(lastPayload, payload, sizeof(payload)) != 0) {
    memcpy(lastPayload, payload, sizeof(payload));
    ws.binaryAll(payload, sizeof(payload));
    printState(decodeState(frame));
  }
}
