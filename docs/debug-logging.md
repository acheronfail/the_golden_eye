# Enabling debug logging

If something isn't working right, the plugin's logs are the best place to start
— and they're what maintainers will ask for in a bug report. The plugin writes
into OBS's own log, so you don't need any extra tools to read them.

By default only higher-level messages are logged. Detailed `debug` messages are
turned off unless you ask for them (see [Turning on debug logs](#turning-on-debug-logs)
below).

## Viewing the logs

In OBS, open **Help → Log Files → View Current Log**. The plugin's messages are
mixed in with OBS's own and are prefixed with `[the_golden_eye]`.

To share a log when reporting an issue, use **Help → Log Files → Upload Current
Log File** — OBS uploads it and gives you a link you can paste into the report.

If you'd rather open the file yourself, the current session's log is the newest
file in:

- **macOS:** `~/Library/Application Support/obs-studio/logs/`
- **Linux (Flatpak):** `~/.var/app/com.obsproject.Studio/config/obs-studio/logs/`
- **Windows:** `%APPDATA%\obs-studio\logs\`

## Turning on debug logs

Debug logging is controlled by an environment variable named `RUST_LOG`. OBS has
to be started with that variable set to `ge_rust=debug`, so the steps below start
OBS from a terminal instead of the usual way. Quit OBS first if it's already
running.

The extra detail lasts only for that session — start OBS normally again and it
goes back to the default level.

### macOS

Open **Terminal** and run:

```sh
RUST_LOG=ge_rust=debug "/Applications/OBS.app/Contents/MacOS/OBS"
```

OBS stays attached to the Terminal window; closing the window (or pressing
`Ctrl+C`) quits OBS.

### Windows

Open **PowerShell** and run:

```powershell
$env:RUST_LOG = "ge_rust=debug"
& "C:\Program Files\obs-studio\bin\64bit\obs64.exe"
```

(Adjust the path if you installed OBS somewhere else.)

### Linux

For the Flatpak install, run:

```sh
flatpak run --env=RUST_LOG=ge_rust=debug com.obsproject.Studio
```

We don't currently support non-flatpak installations of OBS, so you're on your
own here if you are running a different installation.

## Want even more detail?

`ge_rust=debug` covers almost everything. For the most verbose output, swap
`debug` for `trace`:

```
RUST_LOG=ge_rust=trace
```
