# Auto-update compatibility plan

## Goal

The plugin's auto-update flow replaces the hosted core library and selected runtime data, but it
does not replace the OBS-loaded shim. A release that requires a different shim must therefore
require a manual installation.

Encode this compatibility boundary directly in each release package name:

```text
the_golden_eye-u0-v0.6.0-linux-x86_64.zip
               ^^ updater version
                  ^^^^^^ plugin version
```

The installed core supports one updater version compiled into it. A release can be installed
automatically only when its package's updater version exactly matches the installed core's supported
updater version:

```text
target updater version == installed supported updater version
```

A higher, lower, missing, malformed, or ambiguous updater version requires manual installation. The
updater version is independent of the plugin's SemVer version.

Examples:

| Installed support | Release package | Result    |
| ----------------- | --------------- | --------- |
| `u0`              | `u0-v0.6.1`     | Automatic |
| `u0`              | `u1-v0.7.0`     | Manual    |
| `u1`              | `u1-v0.7.1`     | Automatic |
| `u1`              | `u2-v0.8.0`     | Manual    |

When a future release needs a new shim or otherwise changes the installation contract, increment the
updater version before building that release.

## Current rollout status

- PR 1 was merged to `master` as #139 with green CI.
- The original `u1 -> u1` and `u1 -> u2` OBS simulations were manually verified before the numbering
  pivot.
- PR #140 completed the numbering pivot, and the `u0 -> u0` and `u0 -> u1` OBS simulations passed.
- The dual-named `v0.6.1` bridge is published, and its legacy-update and manual-install checks
  passed.
- The one-release alias machinery is now being removed before work begins on `v0.7.0`.
- The shim/path work described below defines updater `u1` and ships in `v0.7.0`; the bridge retains
  the legacy core-only contract as explicit updater `u0`.

## Updater version configuration

Add a checked-in source of truth:

```text
obs2/updater-version.txt
```

The file contains a non-negative integer without the `u` prefix. `0` names the legacy core-only
contract that existed before updater versions were explicit:

```text
0
```

Also support a `GE_UPDATER_VERSION` build-time override. Resolve the value in this order:

1. `GE_UPDATER_VERSION`, when explicitly set.
2. `obs2/updater-version.txt`.
3. Fail the build if neither provides a valid non-negative integer.

Pass the value through the existing build chain:

```text
updater-version.txt / GE_UPDATER_VERSION
    -> justfile
    -> -DGE_UPDATER_VERSION=N
    -> CMake package filename
    -> GE_UPDATER_VERSION passed to Rust
```

Implementation points:

- `justfile` reads the file, exports `GE_UPDATER_VERSION`, and passes it to every CMake
  configuration path, including Flatpak builds.
- `obs2/CMakeLists.txt` validates and caches the value.
- An updater-version stamp, analogous to the plugin-version stamp, ensures that changing the value
  forces the necessary rebuild.
- `obs2/cmake/RustLib.cmake` passes the value into Cargo.
- `obs2/rust/src/lib.rs` exposes the compiled value from `env!("GE_UPDATER_VERSION")`.
- `obs2/cmake/Package.cmake` includes the value in the package basename:

```cmake
${PLUGIN_NAME}-u${GE_UPDATER_VERSION}-v${GE_PLUGIN_VERSION}-${GE_PACKAGE_PLATFORM}-${GE_PACKAGE_ARCH}
```

This produces names such as:

```text
the_golden_eye-u0-v0.6.1-macos-arm64.zip
the_golden_eye-u1-v0.7.0-linux-x86_64.zip
```

## Release asset parsing

Replace the current derived package name in `obs2/rust/src/update_apply.rs` with parsing of the
release assets.

For the current platform, accept one canonical package matching:

```text
the_golden_eye-u<non-negative integer>-v<release version>-<platform>-<architecture>.zip
```

Validate that:

- The embedded plugin version matches the GitHub release tag after normalizing the tag's leading
  `v`.
- The platform and architecture match the running plugin.
- The updater version is a non-negative integer.
- Exactly one canonical package matches the current platform.

Add the parsed compatibility information to `PluginUpdate`:

```rust
pub updater_version: u32,
pub requires_manual_install: bool,
```

`requires_manual_install` is derived from the exact updater-version comparison rather than set
independently.

If the canonical package is missing, malformed, or ambiguous, fail closed and do not download or
stage it.

## Update behavior

When the release updater version matches:

1. Preserve the current automatic or explicit download behavior.
2. Download the canonical package named in the release assets.
3. Verify it using `checksums.txt`.
4. Stage and apply it through the existing reload flow.

When the release updater version does not match:

1. Keep the update phase at `Available`.
2. Never start an automatic download.
3. Reject an explicit `/api/v1/updates/download` request with an incompatibility response.
4. Do not stage or apply any files.
5. Show the manual-install UI.
6. Preserve the user's `autoUpdateEnabled` preference for later compatible releases.

The backend check is authoritative; frontend behavior must not be the only compatibility guard.

## Arbitrary install paths in `u1`

[Issue #119](https://github.com/acheronfail/the_golden_eye/issues/119) identifies the shim's
hardcoded auto-update paths as the remaining installation limitation to remove before 1.0. The
current implementation has three related assumptions:

1. Rust stages next to the canonical core, while the shim independently looks for staging next to
   the shim.
2. `reload.c` infers bundled-data destinations from fixed macOS
   `Contents/MacOS -> Contents/Resources` and Linux/Windows `bin/<arch> -> data` layouts.
3. The downloaded package core is found using the installed core's leaf filename.

`v0.7.0` must remove all three assumptions. Keep the shim responsible only for operations that
cannot survive unloading the Rust core.

The C shim:

- Resolves the canonical core it must load.
- Derives one staging directory beside that canonical core and passes both paths to `ge_core_load`.
- Prechecks, unloads, loads, commits, and rolls back only the core library.
- Removes the staging directory after a successful update.

The Rust core:

- Downloads, verifies, and interprets the release package.
- Finds the platform-standard packaged core and stages it under the installed core's canonical leaf
  name.
- Stages every non-core runtime file needed by the new core.
- Resolves the destination from OBS's existing module data-path bridge.
- Installs staged runtime data transactionally when the new core starts.
- Removes binary-relative fallback paths from `resolve_cv_template_dir`.

Do not add a path manifest or a versioned path struct. Extend `ge_core_load` with the staging
directory alongside the canonical core path. Rust already obtains the OBS data directory through
`ge_obs_module_data_path`; C must not parse, reconstruct, or manipulate that path.

The successful update sequence is:

1. The old Rust core downloads and stages the core and runtime data.
2. The shim prechecks the staged core, unloads the old core, and loads the staged core from its
   temporary copy.
3. During `ge_core_load`, the new Rust core copies runtime data into destination-local temporary
   directories and swaps it into place.
4. Rust returns startup success only after the data transaction succeeds.
5. The shim commits the staged core to the canonical core path and removes staging.

If runtime-data installation fails, Rust restores the previous data and returns startup failure. The
shim then discards the staged core and reopens the unchanged canonical core. The canonical core is
not replaced until the new core and its runtime data have both started successfully.

Implement destination-local runtime-data transactions in Rust so the staging and OBS data
directories may live on different filesystems. The shim must no longer know names such as
`cv_templates` or `locale`, or contain `Contents/Resources`, `../../data`, cross-filesystem copy, or
runtime-data rollback logic.

After this change, auto-update may still require the resolved destinations to be writable, but it
must not depend on where the shim, core, staging directory, or OBS data directory sit relative to
one another.

## Manual-install UI

Add a small manual-update dialog:

> This version requires a manual installation because it updates the OBS plugin loader. Close OBS
> before replacing the plugin. Your settings and run history will be kept.

Actions:

- **Open release page**
- **Later**

Use the update store's existing in-memory dismissed-version behavior. After dismissal, retain a
sticky notification indicating that manual installation is required.

While a manual update is available, the Options page should:

- Preserve but temporarily disable the auto-update checkbox.
- Explain that automatic installation is unavailable for this release.
- Replace **Download now** with **Open release page**.

No new persistent dismissal setting or update lifecycle phase is required.

## Migration from legacy package names

Clients released before this change look for the legacy name:

```text
the_golden_eye-0.6.1-linux-x86_64.zip
```

The first updater-aware bridge release formalizes the existing core-only updater as `u0`. It must
publish two names for every platform package:

```text
# Canonical package for updater-aware clients
the_golden_eye-u0-v0.6.1-linux-x86_64.zip

# Temporary alias for legacy clients
the_golden_eye-0.6.1-linux-x86_64.zip
```

Both files must be included in `checksums.txt`. This allows legacy clients to install the bridge
release.

Starting with the first incompatible release, publish only the canonical package:

```text
the_golden_eye-u1-v0.7.0-linux-x86_64.zip
```

The outcomes are:

- Legacy clients cannot find their expected package and fail before downloading or staging.
- Bridge clients parse `u1`, compare it with their compiled `u0` support, and show the manual
  installation flow.
- A manually installed `u1` plugin can automatically install later `u1` releases.

Legacy aliases must be limited to the bridge release and must not return in later releases.

## Local update simulation

Extend `obs2/scripts/simulate_update.py` with:

```text
--updater-version N
```

Resolve its value in this order:

1. `--updater-version N`
2. `GE_UPDATER_VERSION`
3. `obs2/updater-version.txt`

Forward arbitrary arguments from the `just` recipe:

```text
simulate-update *args:
    python3 obs2/scripts/simulate_update.py {{ args }}
```

Example manual tests when the running bridge plugin supports `u0`:

```sh
# Compatible update: should download, verify, stage, and apply.
just simulate-update --updater-version 0

# Incompatible update: should show the manual-install UI and make no download request.
just simulate-update --updater-version 1
```

The simulator must:

- Build with both `GE_PLUGIN_VERSION` and `GE_UPDATER_VERSION`.
- Locate the exact expected `uN-vX.Y.Z` package instead of choosing the last ZIP found.
- Put that canonical filename in the mock GitHub release response.
- Generate a checksum entry using that exact filename.
- Print the simulated plugin and updater versions.
- Explain whether the requested updater version matches the checked-in updater version.
- Continue serving the package for an incompatible test so an accidental backend download is visible
  and testable.

The simulator temporarily supported a legacy-asset alias while validating the `v0.6.1` bridge. That
option is removed after publication; subsequent simulations expose canonical packages only.

## Release workflow

The `v0.6.1` release workflow:

1. Canonical packages always use the `uN-vX.Y.Z` naming convention.
2. `checksums.txt` contains the canonical packages.
3. Used a one-off release step to create legacy aliases and include them in `checksums.txt`.
4. The workflow verifies that every supported platform has exactly one canonical package.
5. Every canonical package has the updater and plugin versions expected for the release tag.

After the verified bridge release, the workflow must contain no legacy-alias branch: every later
release publishes only its canonical packages and checksums.

## Test plan

### Rust update selection and application

- Parse `u0-v0.6.1` packages for every supported platform and architecture.
- Require the embedded plugin version to match the release tag.
- Installed `u0`, target `u0`: automatic installation.
- Installed `u0`, target `u1`: manual installation.
- Installed `u1`, target `u0`: manual installation.
- Missing or malformed `uN`: fail closed.
- Missing `v` or malformed plugin version: fail closed.
- Multiple canonical packages for the current platform: reject as ambiguous.
- Manual update with auto-update enabled never starts a download.
- Explicit download rejects an incompatible updater version.
- Compatible packages retain checksum, staging, safety, reload, and rollback behavior.

### Browser

- Compatible updates retain the existing download/apply notifications.
- Incompatible updates show the manual-install dialog even when auto-update is enabled.
- Dismissing the dialog retains a sticky manual-update notification.
- The release-page action opens only the validated repository release URL.
- Options preserves but disables the auto-update preference while the update is incompatible.

### Packaging and simulation

- Package names contain the configured updater and plugin versions.
- Changing `obs2/updater-version.txt` forces the affected build outputs to rebuild.
- `GE_UPDATER_VERSION` overrides the checked-in value for local builds.
- Negative or otherwise invalid updater-version configuration fails early.
- The simulator generates and serves the updater version requested on its command line.
- The bridge release contains both canonical and legacy assets.
- Releases after the bridge contain no legacy aliases.

### Arbitrary install paths

- A shim beside the core and data directory retains the normal packaged behavior.
- `GE_CORE_LIB` pointing to an unrelated directory stages and replaces that exact core.
- A custom installed core filename accepts the standard packaged core and preserves the custom
  destination filename.
- The shim passes one explicit staging directory to Rust.
- An OBS data directory unrelated to both shim and core receives `cv_templates` and `locale`.
- Data sync works when the core staging and data destinations are on different filesystems.
- Paths containing spaces are supported; overlong or missing paths fail cleanly.
- macOS, Linux, and Windows path separators are covered without install-layout enums.
- A failed core load restores the old core; a failed Rust data transaction restores the previous
  data before startup reports failure.
- `just dev` still stages and hot-reloads through the production path contract.

## Implementation order

1. Add `obs2/updater-version.txt` and thread its value through `just`, CMake, Cargo, and packaging.
2. Change canonical package naming to `uN-vX.Y.Z`.
3. Parse updater versions from release assets and expose the compatibility result in `PluginUpdate`.
4. Add authoritative download/staging guards for incompatible updates.
5. Add the manual-install dialog, notification behavior, and Options treatment.
6. Update the release workflow with the explicitly enabled one-release legacy alias.
7. Extend the simulator with updater-version selection and exact package lookup.
8. Add unit, integration, frontend, packaging, and simulator coverage.
9. Publish the dual-named `u0` bridge release before publishing an incompatible `u1` release.
10. Remove the one-release legacy alias machinery after `v0.6.1` is verified.
11. Pass the shim's canonical core and staging paths into the Rust core.
12. Move runtime-data installation and rollback into Rust and add arbitrary-path tests.
13. Bump the updater version to `u1` in the same PR as the breaking shim/core ABI.
14. Publish `v0.7.0`, verify custom-location updates, then update issue #119.

## Delivery plan

The work is delivered through the merged implementation PR, one numbering-pivot PR, two later PRs,
and two release milestones. The first release uses `u0` to name the legacy core-only contract. The
cleanup PR removes its one-off bridge code. The final implementation PR removes the shim's hardcoded
install-layout assumptions and pairs that breaking shim/core ABI with the first path-safe updater,
`u1`, in `v0.7.0`.

This rollout does not release `v1.0.0` or declare the plugin's public behavior stable. The updater
version is an installation-format version independent of SemVer. Shipping `u1` in `v0.7.0` resolves
the known manual-install/shim-path prerequisite for 1.0 while leaving storage stability and
matcher-quality goals for later work.

### PR 1: implement updater-version compatibility

Merged as #139. Its implementation initially used `u1` for the bridge; the required follow-up pivot
below changes the unreleased contract to `u0` before the first tagged package.

Create a feature branch from the latest `master`:

```sh
git switch -c codex/updater-version-compatibility
```

Suggested commits:

1. `build: add a configurable updater package version`
   - Add `obs2/updater-version.txt`.
   - Thread `GE_UPDATER_VERSION` through `just`, CMake, Cargo, build stamps, and packaging.
   - Change the canonical package name to `the_golden_eye-uN-vX.Y.Z-<platform>-<arch>.zip`.
   - Add build and packaging validation.
2. `feat: gate auto updates by package updater version`
   - Parse the canonical platform asset.
   - Add updater compatibility to `PluginUpdate`.
   - Block automatic and explicit downloads when updater versions differ.
   - Fail closed for missing, malformed, or ambiguous canonical assets.
3. `feat: show manual installation for incompatible updates`
   - Add the manual-install dialog and sticky notification.
   - Update Options while preserving the saved auto-update preference.
   - Add backend, API, store, component, and route coverage.
4. `test: simulate compatible and incompatible updater versions`
   - Add `--updater-version` and temporary `--legacy-asset-alias` simulator options.
   - Make the simulator find the exact expected package.
   - Add compatible and incompatible manual test instructions.
5. `ci: publish legacy update aliases for v0.6.1`
   - Add the exact `LEGACY_ALIAS_RELEASE_TAG: v0.6.1` release setting.
   - Copy each canonical bridge package to its legacy no-`u`, no-`v` alias.
   - Generate `checksums.txt` only after both sets of assets exist.
   - Assert that aliases are produced only for the configured bridge tag.
6. `docs: document updater-version compatibility`
   - Update `docs/dev/auto-update.md` with the implemented behavior.
   - Keep this plan aligned with any implementation decisions made during review.

Commits may be combined when that makes review easier, but keep the build/package plumbing, runtime
behavior, UI, and one-off release logic independently reviewable.

Before opening the PR, run the relevant repository checks:

```sh
just fmt
just test-rust
just test-integration
just test-shim
cd obs2/browser
npm run check
npm run lint
npm run test
```

Also build a local package and confirm its exact name:

```sh
just make-package
```

Expected canonical basename:

```text
the_golden_eye-u0-v<current-version>-<platform>-<arch>.zip
```

Run both simulator paths against a plugin built with the checked-in `u0`:

```sh
# Terminal 1: compatible target
just simulate-update --updater-version 0

# Terminal 2
just obs
```

Repeat with an incompatible target:

```sh
# Terminal 1: incompatible target
just simulate-update --updater-version 1

# Terminal 2: explicitly restore the running build to u0
GE_UPDATER_VERSION=0 just obs
```

For the incompatible case, verify that:

- The manual-install UI appears.
- No package download is requested from the simulator.
- No `.ge_update_staged` update is created.
- The explicit download endpoint rejects the update.

Open the PR with the release plan called out explicitly:

- Merge target: `master`.
- Next stable tag: `v0.6.1`.
- Checked-in updater version: `u0`.
- Release must contain canonical and legacy package names.
- No other stable `0.x` release should be published after the one-off alias code is removed.

Do not tag `v0.6.1` until the PR is merged and the merge commit has passed the normal branch checks.

### Follow-up PR: reserve `u1` for the path-safe updater

Land this follow-up before tagging `v0.6.1`:

1. Allow updater version zero in CMake, Rust release parsing, the package contract checker, and the
   simulator.
2. Change `obs2/updater-version.txt` from `1` to `0`.
3. Update bridge assets and tests from `u1-v0.6.1` to `u0-v0.6.1`.
4. Update incompatible simulations from `u1 -> u2` to `u0 -> u1`.
5. Keep `u1` reserved for the `v0.7.0` shim that removes the path limitation.

Run the full updater, integration, browser, simulator, and package-contract checks again. This is a
numbering correction only: it must not weaken the exact-match compatibility check or change the
already verified manual-install behavior.

### Release 1: publish the `v0.6.1` bridge

Create and push the tag from the verified `master` commit:

```sh
git switch master
git pull --ff-only
git tag v0.6.1
git push origin v0.6.1
```

The release workflow creates a draft release. Before publishing it, verify:

1. Every supported platform has one canonical package:

   ```text
   the_golden_eye-u0-v0.6.1-<platform>-<arch>.zip
   ```

2. Every supported platform has one legacy alias:

   ```text
   the_golden_eye-0.6.1-<platform>-<arch>.zip
   ```

3. `checksums.txt` contains every canonical package and every legacy alias.
4. The canonical package contains a core compiled with updater version `u0`.
5. The canonical and legacy files for a platform are byte-for-byte identical.
6. No unrelated or stale package is attached.

Download the draft assets into a temporary directory and validate all checksums before publication:

```sh
gh release download v0.6.1 --repo acheronfail/the_golden_eye --dir /tmp/the-golden-eye-v0.6.1
cd /tmp/the-golden-eye-v0.6.1
sha256sum --check checksums.txt
```

On a platform without `sha256sum`, use its SHA-256 verification equivalent.

Publish the draft only after the asset inspection passes:

```sh
gh release edit v0.6.1 --repo acheronfail/the_golden_eye --draft=false
```

After publication, perform two end-to-end checks:

- An installed `v0.6.0` discovers the legacy `v0.6.1` asset and auto-updates successfully.
- An installed `v0.6.1` discovers a simulated `u1` release and requires manual installation without
  downloading it.

Leave `v0.6.1` as the latest stable release long enough to exercise the bridge in normal update
checks before publishing `v0.7.0`.

### PR 2: remove the one-off `v0.6.1` bridge machinery

Create this cleanup branch only after the published bridge release and both end-to-end checks have
passed:

```sh
git switch master
git pull --ff-only
git switch -c codex/remove-v0.6.1-update-alias
```

Suggested commit:

```text
ci: remove the v0.6.1 legacy update bridge
```

Remove:

- `LEGACY_ALIAS_RELEASE_TAG`.
- The release-workflow step that copies canonical packages to legacy aliases.
- Tests that assert legacy aliases are produced.
- The simulator's temporary `--legacy-asset-alias` option and its tests.
- Any build/CMake option used only to create legacy aliases.
- Instructions that suggest future releases may publish legacy names.

Keep:

- `obs2/updater-version.txt`.
- `GE_UPDATER_VERSION` override support.
- Canonical `uN-vX.Y.Z` naming.
- Asset parsing and compatibility enforcement.
- Manual-install UI and tests.
- Historical documentation stating that `v0.6.1` was the only dual-named bridge release.

Run the normal checks and build a package. Verify that it produces only the canonical name and that
`checksums.txt` contains no legacy alias.

Open and merge this cleanup as its own PR so the intentionally temporary release code does not
remain mixed into the long-lived updater implementation. Merge it before publishing `v0.7.0`.

Do not publish a stable `v0.6.2` from the cleaned-up workflow. Once GitHub marks a later stable
release as latest, a legacy `v0.6.0` client will no longer discover the dual-named `v0.6.1` bridge.
If a `0.6.x` hotfix becomes unavoidable, temporarily restore the exact legacy-alias release step for
that hotfix and remove it again afterward.

### PR 3: remove hardcoded shim paths and bump `v0.7.0` to `u1`

The updater-version bump must be committed with the change that actually requires a new shim. Do not
bump it in an unrelated release.

Create a feature branch from cleaned-up `master`:

```sh
git switch master
git pull --ff-only
git switch -c codex/v0.7-shim-and-updater-v1
```

Suggested commits:

1. `refactor: keep the shim reload path core-only`
   - Resolve the canonical core and adjacent staging directory once in `plugin.c`.
   - Pass both paths through `ge_core_load`; do not add a manifest or path-contract struct.
   - Keep C responsible only for core precheck, unload/load, commit, and rollback.
   - Remove C knowledge of OBS data layouts, `cv_templates`, and `locale`.
2. `feat: install runtime update data from Rust`
   - Remove the platform install-layout enum and relative data-path reconstruction.
   - Decouple the packaged core source filename from the installed destination filename.
   - Have the newly loaded Rust core transactionally install staged runtime data before startup
     reports success.
   - Restore previous runtime data in Rust when installation fails so C can reopen the old core.
   - Use destination-local temporary directories so staging and OBS data may be on separate
     filesystems.
   - Remove Rust's binary-relative `cv_templates` fallback paths.
   - Update `dev.py` if needed to use the same staging contract.
   - Add fixture-driven shim tests for unrelated core/data directories, custom filenames, spaces,
     Rust data rollback, and all supported path separators.
3. `build: require updater version 1 for v0.7 packages`
   - Change `obs2/updater-version.txt` from `0` to `1`.
   - Update package and compatibility expectations to `u1`.
   - Keep this commit in the same PR as the shim ABI change so they cannot be released
     independently.
4. `docs: add the v0.7 manual installation instructions`
   - Explain that `u0` installations must manually install the first `u1` package.
   - Explain that later `u1` releases return to normal automatic updates.
   - Update `README.md` so it no longer says the next required manual installation first occurs at
     `v1.0.0`.
   - Document that arbitrary core/data install locations are now supported.
   - Keep the remaining `v1.0.0` stability goals explicitly unreleased and out of scope.

Before opening the PR, run the full relevant test suite and repeat the local simulator matrix:

- Running `u0` against target `u1`: manual installation, no download.
- Manually installed `u1` against a newer target `u1`: normal automatic update.

The PR description must identify the updater-version bump as a release invariant. Review should
reject any build that contains the shim-breaking change while still producing `u0` assets.

### Release 2: publish `v0.7.0`

If prereleases are used, the first `v0.7.0-beta.N` package must already be `u1`; do not defer the
updater-version bump until the stable tag. A user who manually installs a `u1` beta can then
auto-update to later `u1` betas and the stable release.

For the stable release:

```sh
git switch master
git pull --ff-only
git tag v0.7.0
git push origin v0.7.0
```

Before publishing the generated draft, verify:

- Every package is named `the_golden_eye-u1-v0.7.0-<platform>-<arch>.zip`.
- No legacy package names are attached.
- `checksums.txt` contains only the canonical packages.
- The package contains both the `u1` core and the path-safe `u1` shim.
- A `u0` installation reports manual installation.
- A manually installed `u1` package loads and updates when its core and OBS data directory are in
  unrelated custom locations.
- A manually installed `u1` package can auto-update to a simulated newer `u1` release.

Publish the draft only after these checks pass.

### Post-release maintenance

After `v0.7.0` is published:

1. Update `README.md` and `docs/dev/auto-update.md` to describe `u1` as the current updater version.
2. Convert this document from an active rollout plan into historical architecture documentation, or
   move completed delivery steps into a short release-history section.
3. Search for and remove stale `0.6.1`, legacy-alias, and bridge-only references:

   ```sh
   rg -n "0\.6\.1|LEGACY_ALIAS|legacy.asset|legacy alias|bridge release" .
   ```

4. Confirm that no release job can emit the old `the_golden_eye-X.Y.Z-...zip` format.
5. Keep the updater-version mismatch tests permanently; they protect every future `uN -> uN+1`
   boundary.
6. For ordinary releases, leave `obs2/updater-version.txt` unchanged. Increment it only when the
   installed updater cannot safely apply the new package.
7. Update issue #119 after `v0.7.0` is published: mark the shim/path prerequisite as delivered and
   remove the statement that all `0.x` users must wait until `1.0.0` for the manual installation.
   Keep the issue open for storage stability and matcher-quality goals.
