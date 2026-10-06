# Data compatibility

Version `1.0.0` uses schema 3 as the stable run catalog. Future schema changes must preserve run
IDs, history, metadata, retention state, monitoring sessions, and YouTube links through migrations.
The plugin rejects unknown schemas without a reset. Downgrades can fail.

## Updating from 0.x.x

Schema 2 migrates to schema 3 without data loss. Schema 3 keeps its data, including during automatic
updates.

Only schema 1 resets. The update keeps settings and clip files and imports tagged clips as kept
runs. These clips survive automatic cleanup. The reset removes database-only history, monitoring
sessions, and YouTube links.

To keep a copy of schema 1 history, close OBS and back up `runs.sqlite` beside `settings.json`.

## Installation

Version `1.0.0` uses updater `u2`. Existing `u2` installations can update automatically. For older
`u1` installations, close OBS and install the full package, including loader and core. See the
[installation guides](../README.md#how-to-install).
