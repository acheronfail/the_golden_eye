# Startup calibration regression

Captured on 2026-10-11 with the English ROM and The Golden Eye 1.0.5 running in OBS.

- `corrupted-signal.png`: replay frame at 00:02.533 from the signal transition after
  the UGREEN splash, reduced to the initial monitor size (853×480). It includes
  the OBS scene overlays. The orange/blue corruption passed the folder colour
  detector and caused a permanent full-frame 4:3 squeeze.
- `start.png`: the source-only Train Agent dossier captured through the screenshot
  API, reduced from 1920×1080 to 853×480 with FFmpeg's default scaling.
- `squeezed-start.png`: the actual 640×480 monitor input dumped after the bad GPU
  transform was latched. Both languages failed to recognize this frame.

The CV sequence test checks that corruption never commits capture geometry and
that the next real dossier detects English. Runtime tests use the squeezed frame
for the bounded recovery probe and verify that resetting capture discards queued
transformed frames and permits a new calibration.
