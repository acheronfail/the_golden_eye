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
</table>

... and more!

## OBS compatibility

The recommended minimum OBS Studio version is `31.1.0`, but the plugin should work on OBS `31.0.0`
and later.

## How to install

> [!WARNING] Until version `1.0.0`, this plugin is pre-release software with no stability or
> compatibility guarantees. `0.x.x` releases may include breaking changes, including to stored run
> data or the installation format. See
> [Changes for 1.0.0](https://github.com/acheronfail/the_golden_eye/issues/119) for details.

Follow the instructions for your operating system:

- [Install on Linux](docs/install-linux.md)
- [Install on macOS](docs/install-macos.md)
- [Install on Windows](docs/install-windows.md)

## Updates

The plugin has an auto-update feature! Most of the time it can update itself just fine.

If there's ever a new update that requires the plugin to be manually installed, the plugin will let
you know.

## Troubleshooting

If the plugin misbehaves, [enabling debug logging](docs/debug-logging.md) helps you (and
maintainers) see what's going on.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to work on this plugin.
