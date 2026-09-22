# Native OBS plugin

Follow [CONTRIBUTING.md](../CONTRIBUTING.md) for system prerequisites, setup, commands, and tests.
Run all root commands from the repository root, including `just setup`, `just dev`, and `just check`.

## Find the code

- [Runtime ownership](rust/runtime/src/README.md): plugin workflows and native adapters.
- [CV ownership](rust/cv/README.md): calibration, templates, and frame matching.
- [Run monitoring](rust/runtime/src/run_monitoring/README.md): run transitions and recording.
- [Browser ownership](browser/src/lib/README.md): dock components and state.
- [Update contract](../docs/dev/auto-update.md): the resident loader and replaceable core.
