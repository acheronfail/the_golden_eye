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
