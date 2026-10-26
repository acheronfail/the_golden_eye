# OBS compatibility and universal u2 packages

OBS 31/32 and 33 use one `u2` release per platform/architecture. The loader/core contract and
release discovery are unchanged, so an original u2 updater can skip intervening releases and install
this release directly. No bridge release or u3 generation is required.

## Package and update contract

Linux and Windows packages contain a legacy loader under `bin/64bit`, a modern loader at the package
root, and **one** heavy core under `bin/64bit`. Both loaders use the same source and interface;
their compiled relative core paths differ. The modern loader is also bundled under
`data/modern-loader`, because original updaters copy runtime data but intentionally ignore loaders.
The package contract check enforces this layout and the unchanged core exports. macOS retains its
existing bundle layout.

The runtime provisions modern discovery only on a normal startup or after the loader commits a
successful hot update. Provisional startup never publishes a modern installation. Development builds
and explicit `GE_CORE_LIB` overrides skip provisioning.

- Linux: a legacy installation in the current XDG config root creates a complete modern bundle in
  the current XDG data root. Copying uses a private sibling workspace; publication uses
  `RENAME_NOREPLACE`, so another installer cannot be overwritten during publication.
- Windows: the standard plugin directory already contains the core and data. Provisioning adds just
  the modern loader using a temporary file and an atomic, non-replacing hard link.
- Existing modern installations are left alone. Legacy files, settings, and run history are
  retained. OBS 33 prefers modern discovery and skips the same named legacy module; the loader also
  retains its duplicate-module guard.

This is a one-time operation, not synchronization. Later updates affect the active installation. A
retained Linux legacy copy may therefore have a different version from the modern copy; each uses
the normal update workflow when run. Uninstalling requires removing both Linux copies. Permissions
failures leave the plugin operational and retain an actionable dashboard notification; fixing
permissions and restarting retries provisioning. Unknown destinations and symbolic links are not
replaced.

## OBS browser initialization

OBS 33's browser changes require initializing CEF before registering the custom dock. The core uses
OBS Browser's versioned QCef v3 interface, guarded by its exported interface version, with a bounded
wait. OBS 31/32 retain their existing startup path. Unknown future browser interfaces are logged and
are not called through the v3 interface.

## Validation

The repeatable Linux cross-version suite needs OBS 32 installed system-wide and OBS 33 installed for
the current user. Build against the OBS 32 SDK, even when testing OBS 33, to retain the older
runtime ABI:

```sh
just test-obs-upgrade --build-only
# Substitute the printed build directory:
bash .github/scripts/run-obs-headless.sh obs2/build/universal-desktop.log \
  node --experimental-strip-types frame_tests/obs/universal.ts obs2/build/obs-upgrade-builds-EXAMPLE
```

Set `GE_OBS_TEST_ORIGINAL_U2` to an extracted original u2 plugin directory to repeat the direct
old-updater test. Without it, the suite removes the modern loader/resources from the first fixture
to exercise provisioning by the next update. It checks unchanged resident loader hashes, committed
core hashes, OBS 32 restart, OBS 33 duplicate prevention, replay saves and history, modern-only
discovery, fresh modern updates, and permission failure/retry.

CI retains OBS 32 builds and tests, then tests the same artifacts on OBS 33. Linux additionally
exercises embedded CEF rendering, live events, reload/reconnection, and replay saving. Windows uses
isolated OBS 32 and 33 copies for distinct compiled-version upgrades and restarts.

OBS 34 is not available yet. Modern-only discovery tests cover its announced removal of legacy
locations; the final OBS 34 browser and native ABI must still be validated against an actual
release. Installing this plugin update and running it before an OBS 32 → 34 upgrade prepares the
modern location without requiring OBS 33 in between. Users who upgrade OBS first, without ever
running this plugin release, may still need to install the current package manually.

References: [OBS 33 release notes](https://github.com/obsproject/obs-studio/releases/tag/33.0.0),
[legacy plugin locations](https://obsproject.com/kb/legacy-plugin-locations), and the
[OBS Browser v3 interface](https://github.com/obsproject/obs-browser/blob/5b503289c45a/panel/browser-panel.hpp).
