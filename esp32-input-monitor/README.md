# NintendoSpy N64 reader — ESP32

A PlatformIO/Arduino port of NintendoSpy's N64 input reading, scoped only to the Nintendo 64's
single-wire controller protocol. The ESP32 passively sniffs the controller DATA line and prints the
decoded input over serial — it never drives the line.

Ported from the project's AVR firmware ([../firmware/firmware.ino](../firmware/firmware.ino),
`loop_N64`) and the packet layout in [../Readers/Nintendo64.cs](../Readers/Nintendo64.cs).

## How it works

The N64 controller talks over one idle-high open-collector wire. Each bit is a ~4µs pulse:

| Bit | Low  | High |
|-----|------|------|
| `0` | ~3µs | ~1µs |
| `1` | ~1µs | ~3µs |

So after each falling edge, sampling the line ~2µs later reads the bit: a `1` has already returned
high, a `0` is still held low. Each polled frame is the console's 9-bit prefix (`0x01` poll command
`0000_0001` + a `1` stop bit) followed by the controller's 32-bit response.

The AVR-specific bits were replaced for the ESP32: `PIND` reads → direct `GPIO.in` register reads,
hand-counted NOP delays → the Xtensa cycle counter (`xthal_get_ccount`) scaled by `F_CPU`.

## Wiring

| N64 connector | ESP32 |
|---------------|-------|
| GND           | GND   |
| DATA (middle) | GPIO 4 (`N64_PIN` in [src/main.cpp](src/main.cpp)) |
| 3.3V          | — (not needed for sniffing) |

The N64 data line is 3.3V logic with a pull-up on the console side, so it connects directly to an
ESP32 input — no level shifting needed. **Share a common ground** with the console/controller.

To sniff a live console↔controller session, tap the DATA line between them (e.g. with a passthrough
adapter). The console must be polling the controller for frames to appear.

If your data wire is on a different GPIO, change `N64_PIN`. Keep it below GPIO 32 (the fast read path
uses `GPIO.in`) and the CPU at 240 MHz (the bit-bang timing assumes it).

## Live web UI (wireless)

The ESP32 also serves a small web page that mirrors the controller state in real time, so
you don't need a wired serial connection to watch input.

No WiFi credentials are hardcoded — they're configured once via a captive portal:

1. On first boot the ESP32 brings up an open WiFi network named **`N64Spy-Setup`**. Join it from a
   phone/laptop; a captive-portal config page pops up automatically. Pick your network, enter the
   password, and save. The credentials are stored in flash, and the ESP32 reconnects to your
   network automatically on every later boot.
2. Open the serial monitor — it prints the IP it got (and `http://n64spy.local/` via mDNS).
3. Open that address in a browser. The page connects to a WebSocket at `/ws`; the firmware pushes
   a 4-byte binary frame on every state change, and the page lights up the buttons / moves the
   stick.

To move the device to a different network, hold the **BOOT** button (`WIFI_RESET_PIN`, GPIO 0 on
most dev boards) for ~3 seconds — either at power-up or any time during normal operation. That
forgets the saved network and reboots into the setup portal.

The async server runs in its own task (on the other core), so it never disturbs the timing-critical
bit-bang sniff. The setup portal runs only during startup, before that server begins, so the two
never clash over port 80.

The 4-byte wire format (see `packState()` in [src/main.cpp](src/main.cpp) and the bit masks in
[include/web_ui.h](include/web_ui.h)):

| Byte | Bits (MSB→LSB) |
|------|----------------|
| 0    | A, B, Z, START, UP, DOWN, LEFT, RIGHT |
| 1    | –, –, L, R, C-UP, C-DOWN, C-LEFT, C-RIGHT |
| 2    | stick X (int8) |
| 3    | stick Y (int8) |

## Build, upload, monitor

```sh
pio run                 # build
pio run -t upload       # flash
pio device monitor      # serial @ 115200
```

## Output

State is logged on change to keep the serial output readable:

```
[N64] A START stick=(0, 0)
[N64] UP stick=(-42, 118)
```

Unrecognized frames (commands other than the controller-state poll, e.g. rumble/mempak) are ignored.
