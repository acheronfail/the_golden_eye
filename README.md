# The Golden Eye

This is a plugin for OBS Studio that assists with GoldenEye N64 speed-running.

- 🎬 Automatically capture runs
- ⭐ Keep your best clips
- 📉 Track your progress
- 📺 One-click (or automatic) YouTube uploads

<table>
  <tr>
    <th width="33%">Automatic recording</th>
    <th width="33%">Run library</th>
    <th width="33%">Progress statistics</th>
  </tr>
  <tr>
    <td align="center" valign="top"><a href="docs/assets/preview-monitor.png"><img src="docs/assets/preview-monitor.png" alt="Live monitor detecting a Facility run and showing recent results" width="220" height="450"></a></td>
    <td align="center" valign="top"><a href="docs/assets/preview-runs.png"><img src="docs/assets/preview-runs.png" alt="Run library with times, results, and saved clips" width="220" height="450"></a></td>
    <td align="center" valign="top"><a href="docs/assets/preview-statistics.png"><img src="docs/assets/preview-statistics.png" alt="Statistics chart showing personal-best progression" width="220" height="450"></a></td>
  </tr>
  <tr>
    <td colspan="3" align="center"><a href="docs/assets/preview-obs.png"><img src="docs/assets/preview-obs.png" alt="OBS mockup with N64 Capture and the monitor showing matching Runway stats" width="800"></a></td>
  </tr>
</table>

... and more!

## OBS compatibility

The recommended minimum OBS Studio version is `31.1.0`, but the plugin should work on OBS `31.0.0`
and later.

## How to install

Version `1.0.0` establishes the stable run-storage baseline. Future upgrades preserve stored run
data through migrations when the schema changes. See
[data compatibility](docs/data-compatibility.md).

> [!WARNING] Upgrading from `0.x.x` resets the run database. Clip files and settings remain intact,
> and tagged clips are reimported as kept runs. Database-only history, monitoring sessions, and
> YouTube associations are lost. See
> [upgrading to 1.0](docs/data-compatibility.md#upgrading-from-0xx).

Follow the instructions for your operating system:

- [Install on Linux](docs/install-linux.md)
- [Install on macOS](docs/install-macos.md)
- [Install on Windows](docs/install-windows.md)

## Updates

The plugin has an auto-update feature! Most of the time it can update itself just fine.

If there's ever a new update that requires the plugin to be manually installed, the plugin will let
you know.

Version `1.0.0` keeps the `u2` updater contract. Existing `u2` installations can update
automatically; older `u1` installations require a full manual installation with OBS closed.

## Troubleshooting

If the plugin misbehaves, [enabling debug logging](docs/debug-logging.md) helps you (and
maintainers) see what's going on.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to work on this plugin.
