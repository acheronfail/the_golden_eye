# Archives GPU sequence regressions

Captured from the OBS plugin's `capture` source on 2026-10-09, using the English
ROM on Archives / Secret Agent. These are lossless PNG conversions of the actual
853×480 GPU matcher inputs, without further resizing. The plugin was version
1.0.4, which includes the Depot fix.

The manifest is ground truth checked against the visible screens and the user's
controlled playthrough. `select` initially has difficulty 3 because its header
says `Mission`, without an Agent label; after choosing Secret Agent it has
difficulty 1. Statistics show 0:06 and a best time of 10:00.

## Capture coverage

| Capture | Complete frames examined | Expected state |
| --- | ---: | --- |
| Aborted report, no completed objectives | 600 | Archives / Secret Agent / abort |
| Briefing | 459 | Archives / Secret Agent / start |
| Difficulty selection | 179 | Archives / select |
| Select → briefing → intro → gameplay → watch → abort | 3,608 | Ordered screen transitions |
| Statistics | 191 | Archives / Secret Agent / stats, times 6 and 600 |

An initial capture exhausted OBS's small temporary filesystem; incomplete frames
were excluded and difficulty selection and transitions were recaptured. Later
captures continuously moved completed files to larger temporary storage.

The repository retains 11 representative frames, one per distinct screen state.
Stationary and transition tests share the same selection and briefing images.
The two aborted reports differ in objective completion; negative examples cover
a black frame, intro, gameplay, opening watch, and watch menu. Near-identical
capture-noise variants and exact duplicates are excluded. Filenames retain each
capture's original zero-based frame number; gaps are intentional.

The full captures were replayed locally during investigation. The committed
fixtures protect the identified failures and state transitions, but do not claim
to cover every capture variation or provide a universal latency guarantee.

The transition boundaries were visually checked: frames 503–504 still show the
difficulty page with the Secret Agent header; frame 505 first shows the briefing;
frame 707 turns black; frame 1666 first shows the aborted report. Gameplay, watch,
and black frames must remain unknown, with no run identity or times.

## Assertions

Run `just test-rust archives_gpu` to exercise `ge_cv`'s sequence tests. Each retained
frame must match on its first read and on two repeated reads. Tests cover cold
matchers and caches primed from Depot and Dam, then repeat the transition sequence
and statistics without resetting the matcher. They check screen, mission, part,
difficulty, detected language, and both raw and classified times.

Both new tests fail against the original 1.0.4 matcher: a cold aborted frame reads
Part v instead of Part ii, and the transition test misses the difficulty-selected frame. Separately,
replaying all 600 original aborted frames after priming with Depot rejected all
600; the cold briefing replay misread all 459 frames as Part v.

With the fix, the full local replays match all 5,037 complete captured frames
against their expected states. The normal screenshot suite remains necessary for
other levels, ROM languages, analog inputs, cheat menus, and time layouts.
