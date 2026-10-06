# Data compatibility

Version `1.0.0` starts the stable run catalog at schema 4. Future schema changes must preserve run
IDs, history, metadata, retention state, monitoring sessions, and YouTube links through migrations.
The plugin rejects unknown schemas without a reset. Downgrades can fail.

## Updating from 0.x.x

The update resets schemas 1–3 once, including during automatic updates.

- The update keeps settings and clip files.
- The update imports tagged clips from configured run folders as kept runs. These clips survive
  automatic cleanup.
- The update removes database-only history, monitoring sessions, and YouTube links.

To keep a copy of the old history, close OBS and back up `runs.sqlite` beside `settings.json`.

## Installation

Version `1.0.0` uses updater `u2`. Existing `u2` installations can update automatically. For older
`u1` installations, close OBS and install the full package, including loader and core. See the
[installation guides](../README.md#how-to-install).
