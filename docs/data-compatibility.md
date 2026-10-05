# Data compatibility

Version `1.0.0` establishes the current run catalog layout as the stable storage baseline,
identified by schema version 4. The schema number is independent of the plugin version and updater
contract.

Future upgrades preserve run IDs, run history and metadata, retention state, monitoring sessions,
and YouTube associations through explicit migrations when needed. The internal table layout may
change. Unsupported future schemas are rejected without resetting their data; downgrades to older
plugins are not guaranteed to work.

## Upgrading from 0.x.x

The first upgrade to `1.0.0` resets pre-1.0 catalog schemas 1–3 and creates the stable baseline.
There are no migrations from these experimental schemas.

- Settings and clip files remain intact.
- Tagged clips in configured run folders are reimported as kept runs.
- Database-only run history, monitoring sessions, and YouTube associations are lost.
- Previously pending clips that are reimported become kept clips and survive automatic cleanup.

Close OBS and back up `runs.sqlite` and your clips before upgrading if you want to retain a copy of
the old history. The database is stored beside `settings.json` in the application config directory:

- macOS: `~/Library/Application Support/The Golden Eye/`
- Linux: `$XDG_CONFIG_HOME/the-golden-eye/`, or `~/.config/the-golden-eye/`
- Windows: `%APPDATA%\The Golden Eye\`

The reset is committed in one database transaction. Later openings of the baseline catalog retain
its data. Existing tagged clips can also be reimported through the Runs reload action.

## Plugin installation

Version `1.0.0` retains updater contract `u2`. Existing `u2` installations can update automatically.
Older `u1` installations require a full manual installation, including both loader and core, with
OBS closed. Follow the [platform installation instructions](../README.md#how-to-install).

The manual installation requirement and the one-time database reset are independent: an automatic
upgrade from a pre-1.0 `u2` installation still resets its old database.
