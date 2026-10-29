# OBS compatibility

OBS 31–33 use the same `u2` package per platform and architecture. Existing updaters can install it
without an intermediate release. See the [Linux](install-linux.md), [Windows](install-windows.md),
and [macOS](install-macos.md) guides for installation.

## Package layout

Linux and Windows packages contain two loaders sharing one core:

- `bin/64bit/`: legacy loader and core.
- Package root: modern loader for OBS 33.
- `data/modern-loader/`: a copy of the modern loader for existing updaters, which replace core and
  data files but leave loaders untouched.

The loader/core interface and update format are unchanged. Package checks enforce this contract.
macOS keeps its existing layout.

## Automatic preparation

At startup or after an update commits, the plugin prepares modern discovery once:

- Linux copies the legacy installation from the XDG config root to the XDG data root.
- Windows adds the modern loader beside the existing `bin` and `data` folders.

Publication is atomic and never replaces an existing installation or follows symbolic links.
Preparation is skipped during provisional update startup, in development builds, and with a
`GE_CORE_LIB` override. Failures leave the plugin running and show a warning linking to installation
instructions.

Linux copies are independent: later updates affect only the active installation, and uninstalling
requires removing both.

## Validation

CI builds against OBS 32 and tests the artifacts on OBS 32 and 33. The Linux cross-version suite
checks preparation, duplicate prevention, updates, restarts, downgrade, run history, replay saving,
and failure/retry. Windows runs update and restart tests on both OBS versions.

To run the cross-version suite locally, install OBS 32 system-wide and OBS 33 for the current user,
then build using the OBS 32 SDK:

```sh
just test-obs-upgrade --build-only
# Substitute the printed build directory:
bash .github/scripts/run-obs-headless.sh obs2/build/universal-desktop.log \
  node --experimental-strip-types frame_tests/obs/universal.ts obs2/build/obs-upgrade-builds-EXAMPLE
```

Optionally set `GE_OBS_TEST_ORIGINAL_U2` to an extracted original u2 installation to test its
updater; otherwise, the suite uses a fixture with the modern loader removed.
