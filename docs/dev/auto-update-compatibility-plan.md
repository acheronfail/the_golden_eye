# Auto-update compatibility plan

## Goal

The plugin's auto-update flow replaces the hosted core library and selected runtime data, but it
does not replace the OBS-loaded shim. A release that requires a different shim must therefore
require a manual installation.

Encode this compatibility boundary directly in each release package name:

```text
the_golden_eye-u1-v0.6.0-linux-x86_64.zip
               ^^ updater version
                  ^^^^^^ plugin version
```

The installed core supports one updater version compiled into it. A release can be installed
automatically only when its package's updater version exactly matches the installed core's
supported updater version:

```text
target updater version == installed supported updater version
```

A higher, lower, missing, malformed, or ambiguous updater version requires manual installation.
The updater version is independent of the plugin's SemVer version.

Examples:

| Installed support | Release package | Result |
| --- | --- | --- |
| `u1` | `u1-v0.6.1` | Automatic |
| `u1` | `u2-v0.7.0` | Manual |
| `u2` | `u2-v0.7.1` | Automatic |
| `u2` | `u3-v0.8.0` | Manual |

When a future release needs a new shim or otherwise changes the installation contract, increment
the updater version before building that release.

## Updater version configuration

Add a checked-in source of truth:

```text
obs2/updater-version.txt
```

The file contains a positive integer without the `u` prefix:

```text
1
```

Also support a `GE_UPDATER_VERSION` build-time override. Resolve the value in this order:

1. `GE_UPDATER_VERSION`, when explicitly set.
2. `obs2/updater-version.txt`.
3. Fail the build if neither provides a valid positive integer.

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
the_golden_eye-u1-v0.6.1-macos-arm64.zip
the_golden_eye-u2-v0.7.0-linux-x86_64.zip
```

## Release asset parsing

Replace the current derived package name in `obs2/rust/src/update_apply.rs` with parsing of the
release assets.

For the current platform, accept one canonical package matching:

```text
the_golden_eye-u<positive integer>-v<release version>-<platform>-<architecture>.zip
```

Validate that:

- The embedded plugin version matches the GitHub release tag after normalizing the tag's leading
  `v`.
- The platform and architecture match the running plugin.
- The updater version is a positive integer.
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

The first `u1` bridge release must publish two names for every platform package:

```text
# Canonical package for updater-aware clients
the_golden_eye-u1-v0.6.1-linux-x86_64.zip

# Temporary alias for legacy clients
the_golden_eye-0.6.1-linux-x86_64.zip
```

Both files must be included in `checksums.txt`. This allows legacy clients to install the bridge
release.

Starting with the first incompatible release, publish only the canonical package:

```text
the_golden_eye-u2-v0.7.0-linux-x86_64.zip
```

The outcomes are:

- Legacy clients cannot find their expected package and fail before downloading or staging.
- Bridge clients parse `u2`, compare it with their compiled `u1` support, and show the manual
  installation flow.
- A manually installed `u2` plugin can automatically install later `u2` releases.

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

Example manual tests when the running plugin supports `u1`:

```sh
# Compatible update: should download, verify, stage, and apply.
just simulate-update --updater-version 1

# Incompatible update: should show the manual-install UI and make no download request.
just simulate-update --updater-version 2
```

The simulator must:

- Build with both `GE_PLUGIN_VERSION` and `GE_UPDATER_VERSION`.
- Locate the exact expected `uN-vX.Y.Z` package instead of choosing the last ZIP found.
- Put that canonical filename in the mock GitHub release response.
- Generate a checksum entry using that exact filename.
- Print the simulated plugin and updater versions.
- Explain whether the requested updater version matches the checked-in updater version.
- Continue serving the package for an incompatible test so an accidental backend download is
  visible and testable.

Temporarily support:

```text
--legacy-asset-alias
```

This exercises the one bridge-release scenario where both canonical and legacy names are published.
Remove this option in the post-`0.6.1` cleanup PR.

## Release workflow

Update the release workflow so that:

1. Canonical packages always use the `uN-vX.Y.Z` naming convention.
2. `checksums.txt` contains the canonical packages.
3. A one-off `LEGACY_ALIAS_RELEASE_TAG: v0.6.1` setting enables a release step that creates legacy
   aliases and includes them in `checksums.txt`.
4. Normal releases cannot accidentally publish legacy aliases.
5. The workflow verifies that every supported platform has exactly one canonical package.
6. Every canonical package has the updater and plugin versions expected for the release tag.

The alias step must require an exact match with `LEGACY_ALIAS_RELEASE_TAG`; it must not run for a
version range or all `0.x` releases. Remove the setting and the alias step after `v0.6.1` has been
published and verified.

## Test plan

### Rust update selection and application

- Parse `u1-v0.6.1` packages for every supported platform and architecture.
- Require the embedded plugin version to match the release tag.
- Installed `u1`, target `u1`: automatic installation.
- Installed `u1`, target `u2`: manual installation.
- Installed `u2`, target `u1`: manual installation.
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
- Invalid updater-version configuration fails early.
- The simulator generates and serves the updater version requested on its command line.
- The bridge release contains both canonical and legacy assets.
- Releases after the bridge contain no legacy aliases.

## Implementation order

1. Add `obs2/updater-version.txt` and thread its value through `just`, CMake, Cargo, and packaging.
2. Change canonical package naming to `uN-vX.Y.Z`.
3. Parse updater versions from release assets and expose the compatibility result in
   `PluginUpdate`.
4. Add authoritative download/staging guards for incompatible updates.
5. Add the manual-install dialog, notification behavior, and Options treatment.
6. Update the release workflow with the explicitly enabled one-release legacy alias.
7. Extend the simulator with updater-version selection and exact package lookup.
8. Add unit, integration, frontend, packaging, and simulator coverage.
9. Publish the dual-named `u1` bridge release before publishing an incompatible `u2` release.

## Delivery plan

The work is delivered through three PRs and two release milestones. The first PR creates and ships
the `u1` bridge. The second removes all one-off bridge code after that release. The third pairs the
next shim change with the `u2` compatibility boundary in `v0.7.0`.

This rollout does not release `v1.0.0` or declare the plugin's public behavior stable. The updater
version is an installation-format version independent of SemVer. Shipping `u2` in `v0.7.0` resolves
one of the known prerequisites for 1.0 while leaving the other 1.0 goals for later work.

### PR 1: implement updater-version compatibility and the `u1` bridge

Create a feature branch from the latest `main`:

```sh
git switch -c codex/updater-version-compatibility
```

Suggested commits:

1. `build: add a configurable updater package version`
   - Add `obs2/updater-version.txt` with `1`.
   - Thread `GE_UPDATER_VERSION` through `just`, CMake, Cargo, build stamps, and packaging.
   - Change the canonical package name to `the_golden_eye-u1-vX.Y.Z-<platform>-<arch>.zip`.
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
   - Add compatible `u1` and incompatible `u2` manual test instructions.
5. `ci: publish legacy update aliases for v0.6.1`
   - Add the exact `LEGACY_ALIAS_RELEASE_TAG: v0.6.1` release setting.
   - Copy each canonical `u1-v0.6.1` package to its legacy no-`u`, no-`v` alias.
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
the_golden_eye-u1-v<current-version>-<platform>-<arch>.zip
```

Run both simulator paths against a plugin built with the checked-in `u1`:

```sh
# Terminal 1: compatible target
just simulate-update --updater-version 1

# Terminal 2
just obs
```

Repeat with an incompatible target:

```sh
# Terminal 1: incompatible target
just simulate-update --updater-version 2

# Terminal 2: explicitly restore the running build to u1
GE_UPDATER_VERSION=1 just obs
```

For the incompatible case, verify that:

- The manual-install UI appears.
- No package download is requested from the simulator.
- No `.ge_update_staged` update is created.
- The explicit download endpoint rejects the update.

Open the PR with the release plan called out explicitly:

- Merge target: `main`.
- Next stable tag: `v0.6.1`.
- Checked-in updater version: `u1`.
- Release must contain canonical and legacy package names.
- No other stable `0.x` release should be published after the one-off alias code is removed.

Do not tag `v0.6.1` until the PR is merged and the merge commit has passed the normal branch checks.

### Release 1: publish the `v0.6.1` bridge

Create and push the tag from the verified `main` commit:

```sh
git switch main
git pull --ff-only
git tag v0.6.1
git push origin v0.6.1
```

The release workflow creates a draft release. Before publishing it, verify:

1. Every supported platform has one canonical package:

   ```text
   the_golden_eye-u1-v0.6.1-<platform>-<arch>.zip
   ```

2. Every supported platform has one legacy alias:

   ```text
   the_golden_eye-0.6.1-<platform>-<arch>.zip
   ```

3. `checksums.txt` contains every canonical package and every legacy alias.
4. The canonical package contains a core compiled with updater version `u1`.
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
- An installed `v0.6.1` discovers a simulated `u2` release and requires manual installation without
  downloading it.

Leave `v0.6.1` as the latest stable release long enough to exercise the bridge in normal update
checks before publishing `v0.7.0`.

### PR 2: remove the one-off `v0.6.1` bridge machinery

Create this cleanup branch only after the published bridge release and both end-to-end checks have
passed:

```sh
git switch main
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
If a `0.6.x` hotfix becomes unavoidable, temporarily restore the exact legacy-alias release step
for that hotfix and remove it again afterward.

### PR 3: make the next shim change and bump `v0.7.0` to `u2`

The updater-version bump must be committed with the change that actually requires a new shim. Do
not bump it in an unrelated release.

Create a feature branch from cleaned-up `main`:

```sh
git switch main
git pull --ff-only
git switch -c codex/v0.7-shim-and-updater-v2
```

Suggested commits:

1. `feat: support the new plugin install layout`
   - Implement the planned shim/path change that requires manual installation.
   - Add or update shim tests for the new behavior.
2. `build: require updater version 2 for v0.7 packages`
   - Change `obs2/updater-version.txt` from `1` to `2`.
   - Update package and compatibility expectations to `u2`.
   - Keep this commit adjacent to the shim change so they cannot be released independently.
3. `docs: add the v0.7 manual installation instructions`
   - Explain that `u1` installations must manually install the first `u2` package.
   - Explain that later `u2` releases return to normal automatic updates.
   - Update `README.md` so it no longer says the next required manual installation first occurs at
     `v1.0.0`.
   - Keep the remaining `v1.0.0` stability goals explicitly unreleased and out of scope.

Before opening the PR, run the full relevant test suite and repeat the local simulator matrix:

- Running `u1` against target `u2`: manual installation, no download.
- Manually installed `u2` against a newer target `u2`: normal automatic update.

The PR description must identify the updater-version bump as a release invariant. Review should
reject any build that contains the shim-breaking change while still producing `u1` assets.

### Release 2: publish `v0.7.0`

If prereleases are used, the first `v0.7.0-beta.N` package must already be `u2`; do not defer the
updater-version bump until the stable tag. A user who manually installs a `u2` beta can then
auto-update to later `u2` betas and the stable release.

For the stable release:

```sh
git switch main
git pull --ff-only
git tag v0.7.0
git push origin v0.7.0
```

Before publishing the generated draft, verify:

- Every package is named `the_golden_eye-u2-v0.7.0-<platform>-<arch>.zip`.
- No legacy package names are attached.
- `checksums.txt` contains only the canonical packages.
- The package contains both the `u2` core and the new shim.
- A `u1` installation reports manual installation.
- A manually installed `u2` package can auto-update to a simulated newer `u2` release.

Publish the draft only after these checks pass.

### Post-release maintenance

After `v0.7.0` is published:

1. Update `README.md` and `docs/dev/auto-update.md` to describe `u2` as the current updater version.
2. Convert this document from an active rollout plan into historical architecture documentation,
   or move completed delivery steps into a short release-history section.
3. Search for and remove stale `0.6.1`, legacy-alias, and bridge-only references:

   ```sh
   rg -n "0\.6\.1|LEGACY_ALIAS|legacy.asset|legacy alias|bridge release" .
   ```

4. Confirm that no release job can emit the old `the_golden_eye-X.Y.Z-...zip` format.
5. Keep the updater-version mismatch tests permanently; they protect every future `uN -> uN+1`
   boundary.
6. For ordinary releases, leave `obs2/updater-version.txt` unchanged. Increment it only when the
   installed updater cannot safely apply the new package.
7. Update the 1.0 tracking issue to mark the shim/path prerequisite as delivered in `v0.7.0`,
   without closing or releasing the remaining 1.0 goals.
