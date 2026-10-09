# Frame regression tests

For tests through real OBS rendering, capture, and replay saves, see
[the OBS test guide](obs/README.md).

This harness runs the Rust `test_match` CLI against PNG fixtures. It derives expected results from
filenames and writes `test_results.json`. The Rust tests also use fixtures under `clips/` and
`screenshots-*`.

The CLI resizes each fixture before detection to match the first OBS capture: at most 480 pixels
high, with the aspect ratio preserved. Smaller fixtures keep their original size. All detectors
receive this frame, including the mission-digit, black-frame, and watch detectors.

This emulates capture dimensions with OpenCV resizing. It does not test OBS GPU rendering or the
crop correction that OBS applies to later frames after calibration. The benchmark suite defaults to
OBS capture emulation and can use a separate calibration frame.

Run from the repository root:

```sh
just test-cv            # build the release matcher and run every frame case
just test-cv flicker    # filter fixture filenames with a regex
just bench-cv          # benchmark unique frame scenarios
```

After a release build, run `npm run test-watch` in this directory to repeat tests when the harness
changes. This command does not rebuild Rust. Rebuild with `just make-release` after matcher changes.

Add PNG fixtures to the appropriate `screenshots-*` capture-source directory. See `screenshots.ts`
for filename parsing and `frames.test.ts` for assertions. Keep filenames descriptive because they
serve as test expectations.

`screenshots-retrogem/en - start - 07 - Agent - language-switch.png` was captured from OBS on
2026-10-09 while the monitor remained in JP mode on the English Frigate Agent start screen. The CV
unit regression checks both area and bilinear downscaling with cold and warmed matchers, including
the reverse language switch. Bilinear downscaling with a warmed JP matcher reproduces the missed
language detection on the original code. The companion `language-switch-gpu-capture.png` fixture
contains the actual 853×480 monitor input captured through the frame-dump API.

Language detection now probes PREVIOUS before the header gate, checking both languages at the
cached tab scale each frame. If that fails, it searches nearby scales at most once per 250 ms
(immediately for a new resolution). A recognized header can also refine a missed tab search.
Level/time parsing still requires the header; language switching does not.

On 2026-10-09, release benchmarks compared the original matcher, the alternate-colon fallback,
and the tab-first implementation. Median matcher times exclude OBS rendering and capture:

| Frame         | Original | Colon fallback | Tab first |
| ------------- | -------- | -------------- | --------- |
| EN gameplay   | 3.41 ms  | 6.36 ms        | 4.90 ms   |
| EN level grid | 2.59 ms  | 5.43 ms        | 3.81 ms   |
| JP level grid | 4.65 ms  | 7.88 ms        | 6.09 ms   |
| EN start      | 4.68 ms  | 4.63 ms        | 4.52 ms   |
| JP statistics | 9.42 ms  | 9.57 ms        | 9.53 ms   |

The grouped harness used 150 samples and 10 warmups per scenario with OBS capture emulation:

```sh
cd frame_tests
GE_CV_BENCH_SAMPLES=150 GE_CV_BENCH_WARMUPS=10 \
  npm run bench -- '^retrogem/(en|jp)/(unknown|levels|start|stats)$'
```

Gameplay was measured separately with the same CLI benchmark, 300 samples, 10 warmups, and
`en - start - 07 - Agent - language-switch.png` priming the cache before
`en - unknown - gameplay - not-black-frame.png`. Tab-first gameplay p95 was 6.37 ms and maximum
12.28 ms. Periodic recovery still creates slower frames; these unpaced measurements do not model
its exact frequency at 60 FPS or guarantee timings on other machines.

See [CONTRIBUTING.md](../CONTRIBUTING.md) for setup and the other test suites.

## Stats-screen performance capture

`screenshots-obs-developer/en - stats - 06 - Agent - 0153_0300_0144.png` is the
losslessly converted developer-tab BMP captured on 2026-10-01. The three supplied
BMP downloads were identical, so only one fixture is retained. It reads Silo Agent,
1:53, target 3:00, best 1:44.

After building the matcher, reproduce a warmed OBS-size benchmark from the repository root:

```sh
GE_CV_BENCH=100 GE_CV_BENCH_WARMUPS=5 GE_CV_BENCH_JSON=1 \
GE_CV_BENCH_CAPTURE=obs \
GE_CV_BENCH_WARM='frame_tests/screenshots-obs-developer/en - stats - 06 - Agent - 0153_0300_0144.png' \
obs2/rust/target/release/test_match en \
  'frame_tests/screenshots-obs-developer/en - stats - 06 - Agent - 0153_0300_0144.png'
```

Use `target/debug/test_match` for the development matcher. The CV crate now uses
optimization in dev builds while retaining debug symbols; other crates keep their
normal debug profile. Set `GE_CV_DIAGNOSTICS=1` to include developer annotations.

The benchmark now includes watch detection, which previously went unmeasured and
cost about 12 ms in unoptimized debug builds. On the development host, 100 warmed
samples of the complete CV path after tab-scale reuse, before the watch optimization
below, gave these results (all below the 16.67 ms budget):

| Build | Annotations | Median | p95 | Maximum |
| --- | --- | --- | --- | --- |
| Dev | Off | 11.61 ms | 12.73 ms | 13.58 ms |
| Dev | On | 13.48 ms | 15.16 ms | 16.25 ms |
| Release | Off | 10.77 ms | 12.63 ms | 13.18 ms |
| Release | On | 13.35 ms | 14.71 ms | 15.22 ms |

These measurements exclude OBS rendering/capture and do not guarantee performance
on other hardware or on the first uncached frame. Before scale reuse, matching
plus black-frame detection alone took 35.89 ms (debug) / 32.66 ms (release) median.

The expensive path was repeatedly sweeping both languages at 13 scales to confirm
the START tab was absent. PREVIOUS now supplies the scale for both vertical tabs;
its cached scale is checked on each frame, with a recovery sweep when confidence
is insufficient. Independent recovery candidates run concurrently. Timing logs
separate `previous tab language` and `screen validation` from digit recognition.

Two full-suite experiments bounded scale reuse: forcing the header-colon scale on
both tabs failed 748 checks; forcing PREVIOUS's scale on labels and digits failed
622 checks. The retained tab-only sharing passed all 17,849 checks. Small glyphs
retain their existing scale handling until the template families can be normalized.

### Watch detection

Watch geometry is cached per thread for the latest image stride and active-picture
rectangle. The detector scans contiguous ring and face spans, preserving every
sampled pixel and threshold. Changes to the stride or crop rebuild the geometry;
no pixels or classification results are cached. The first frame still pays for
geometry construction, and the cache retains only one geometry per thread.

Using the command above with the stats capture and the watch fixtures below,
100 warmed samples gave these median `watch_runtime_ms` values:

| Capture | Dev before / after | Release before / after |
| --- | --- | --- |
| Stats | 1.204 / 0.584 ms | 1.187 / 0.296 ms |
| English clock face | 1.719 / 0.695 ms | 1.618 / 0.391 ms |
| Japanese menu | 1.275 / 0.458 ms | 1.287 / 0.274 ms |

The watch fixtures are `screenshots-yt-rt4kce/en - unknown - 1 - watch-clock-face.png`
and `screenshots-yt-rt4kce/jp - unknown - 1 - watch-menu-surface.png`. All 600 before/after
watch signals matched exactly, including percentages. Differential Rust tests compare
the cached scan with the original full-frame scan across changing pixel data, odd
sizes, crop offsets, clipped regions, and invalid buffers.

Language regressions in `frames.test.ts` run every dossier fixture with the opposite template language,
verify safe rejection plus the detected language, and rerun with that language to check the normal
screen and time expectations. These include difficulty selection, 007 options, reports, and
statistics, not only the start page.

The `en-previous.png` and `jp-previous.png` templates are grayscale crops from RetroGEM screenshots.
The English crop comes from `en - start - 06 - 00 Agent.png`; the Japanese crop comes from
`jp - start - 01 - Agent.png`. The Japanese capture is horizontally stretched, so its crop is
normalized from 81 to 70 pixels wide to match the template geometry. Neither template uses an analog
or cheap HDMI-converter capture.

Regenerate the templates from the repository root with ImageMagick:

```sh
magick "frame_tests/screenshots-retrogem/en - start - 06 - 00 Agent.png" +repage \
  -crop 70x280+1513+750 +repage -grayscale Rec601Luma obs2/cv_templates/en-previous.png
magick "frame_tests/screenshots-retrogem/jp - start - 01 - Agent.png" +repage \
  -crop 81x280+1605+755 +repage -resize '70x280!' -grayscale Rec601Luma obs2/cv_templates/jp-previous.png
```

### Cheat-menu header rejection

RetroGEM's Japanese PP7 gold cheat menu matched unrelated text as header colons.
That admitted the frame to mission recovery and a full part-label scale sweep,
which alone cost about 62 ms despite there being no part label to find.
The header gate now requires two colon hits separated by 0.55–2.60 glyph heights:
distinct header rows, allowing a missing middle row and the mission reader's
existing spacing tolerance. Same-line and widely separated text cannot open it.

On the development host, 25 warmed release samples with OBS capture emulation
reduced the complete CV pipeline median from 82.55 to 2.82 ms, and p95 from
85.67 to 2.99 ms. All 17,849 frame checks pass. Synthetic row-spacing tests and
a cheat-menu/dossier transition test protect rejection and subsequent recovery.
A paired before/after run of all 80 scenarios (50 samples each, alternating
executable order) confirmed 82.42 to 2.95 ms for PP7 gold. The other 79 scenarios'
median ratio was 0.99; the largest median increase was 0.36 ms.
Cold matching still sweeps other scales and can admit false header candidates;
these warmed results do not establish a first-frame latency bound.

English RetroGEM cheat-menu fixtures cover Gold PP7, 2x Laser, and the shorter
list with 2x Throwing Knife selected. These are pixel-identical PNG conversions
of the supplied 1920×1080 BMP captures. All expect `unknown`, since cheat menus
are not run screens. Benchmark each fixture explicitly when checking coverage:
`bench-cv` selects only one representative per capture source/language/screen.

Individual release benchmarks used 100 samples, OBS capture emulation, and
`en - start - 01 - 00 Agent.png` for calibration (five target warmups):

| English cheat selection | Warm median | Warm p95 | Uncalibrated median |
| --- | --- | --- | --- |
| Gold PP7 | 3.02 ms | 3.82 ms | 139.04 ms |
| 2x Laser | 2.95 ms | 3.29 ms | 136.94 ms |
| 2x Throwing Knife | 2.93 ms | 3.60 ms | 143.01 ms |

All samples correctly returned `unknown`, with no times or black-frame detection.
The full fixture suite passed 17,861 checks. The uncalibrated runs omitted the
calibration frame and target warmups; their expensive recovery repeats until a
real dossier supplies the overlay scale, rather than being only a first-frame cost.

### Depot Secret Agent GPU capture

The `depot-native` and `depot-gpu-capture` English Depot Secret Agent start fixtures
were captured from the running OBS plugin on 2026-10-09. The former is the full
1920×1080 source; the latter is the actual 853×480 input saved through the frame-dump
API, converted losslessly from BMP. OpenCV area downscaling alone did not reproduce
the live failure.

The original matcher rejected the GPU header with a warmed scale cache and could
misread the mission/difficulty with a cold cache. Header colons now use lighter
Gaussian smoothing; mission anchoring admits slightly weaker colons and searches
alternative anchors when the preferred fixed-slot digit fails. Header row-spacing
and confidence requirements remain intact. Time colons retain their original blur
so analog-capture time recognition is unchanged.

The CV unit regression checks both fixtures with area and bilinear resizing,
cold caches, Dam-primed caches, and native-Depot-primed caches. Repeated reads also
cross English cheat-menu and gameplay frames before returning to Depot. The frame
harness checks both fixtures, including opposite-language rejection and recovery.
