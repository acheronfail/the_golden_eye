# CV code ownership

The crate detects GoldenEye overlays, black frames, and the in-game watch. Its public API stays in
`src/lib.rs`; callers do not depend on the private module layout.

| Module              | Responsibility                                                    |
| ------------------- | ----------------------------------------------------------------- |
| `config`            | Runtime flags and the template directory                          |
| `calibration`       | Picture bounds, aspect correction, and coordinate mapping         |
| `match_result`      | Screen classifications, match results, and annotations            |
| `template_matching` | Template loading, scaling, correlation, and detection suppression |
| `overlay_reading`   | Header colons, mission digits, and level times                    |
| `matcher`           | Template caches and the per-frame matching workflow               |
| `parallel`          | Ordered work across scoped threads                                |
| `black_frame`       | Black-frame detection                                             |
| `watch`             | Watch detection and pause transitions                             |
| `timer`             | Optional timing diagnostics                                       |

`cv_test.rs` checks public behavior and the calibration and screen-validation rules. The frame
regression harness checks the matcher CLI against captured images. Run `just test-rust` for Rust
tests and `just test-cv` for frame regressions from the repository root.

Folder colour produces a tentative calibration; a recognized PREVIOUS tab (in either language)
must confirm it before the matcher caches it or offers it to GPU capture. This prevents transient
signal corruption from changing subsequent frames. The monitor retries original capture geometry
after two seconds of unknown results when a fresh folder candidate is visible, discarding the old
matcher and queued transformed frames. Recognized screens or languages reset that recovery timer.
