# Startup calibration regression

Captured on 2026-10-11 with the English ROM and The Golden Eye 1.0.5 running in OBS.

- `corrupted-signal.png`: the N64-source portion of the replay frame at
  00:02.533, during the signal transition after the UGREEN splash. The saved OBS
  scene placed the source at (470, 0), scale 1, with 230 pixels cropped from the
  left and 240 from the right. We extracted the 1450×1080 rectangle at (470, 0)
  and restored the cropped columns as black bars (confirmed by the capture's
  owner), giving the original 1920×1080 source layout. It was then reduced to
  853×480, the monitor's initial input size; no OBS overlays remain. This is a
  reconstruction from the encoded replay, not a lossless monitor frame dump.
  Its corruption independently passes the old folder colour and stretch rules.
- `start.png`: the source-only Train Agent dossier captured through the screenshot
  API, reduced from 1920×1080 to 853×480 with FFmpeg's default scaling.
- `squeezed-start.png`: the actual 640×480 monitor input dumped after the bad GPU
  transform was latched. Both languages failed to recognize this frame.

The CV sequence test verifies that the source corruption passes the old folder
and stretch rules, then checks that it never commits capture geometry and
that the next real dossier detects English. Runtime tests use the squeezed frame
for the bounded recovery probe and verify that resetting capture discards queued
transformed frames and permits a new calibration.
