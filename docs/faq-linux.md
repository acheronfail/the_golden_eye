# Troubleshooting on Linux

These instructions are for OBS Studio installed through Flatpak.

They should be similar for non-Flatpak installations, but this project doesn't support those OBS
installations!

## Computer slows down while using the plugin.

Saving a long replay buffer can briefly stall other applications, especially when OBS writes to the
same disk as the operating system, since OBS saves the entire buffer before the plugin trims the run
and deletes it shortly after.

Recording at 10 Mb/s, a 20-minute buffer is roughly 1.5 GB per save; so disk contention (manifested
as laggy software) can occur if the plugin is writing to the same disk as your applications/distro
is using.

Some recommendations for how to get around this:

### Use a separate drive (recommended)

Set OBS's **Settings → Output → Recording → Recording Path** to a folder on a separate hard drive
from your computer's main disk.

### Set up RAM-backed replay storage

If another drive is unavailable or replay saves still cause stalls, and you have enough spare RAM,
try `/dev/shm` as an alternative. This keeps temporary replay writes in RAM while finished clips are
saved to disk.

Set OBS's **Settings → Output → Recording → Recording Path** to `/dev/shm` - this folder is RAM only
(never writes to disk).

Then set the plugin's clip save directory to wherever you'd like to save your clips - make sure it's
also not at `/dev/shm`, since this is backed by RAM - if you reboot your computer they're lost
forever.

## OBS reports “unspecified error while recording”

First, check **Help → Log Files → View Current Log**.

If you see `No space left on device` that means the folder for your clips is full - i.e. the drive
you're using is out of space.

If you see heaps of saved buffer replays that you didn't save yourself, check OBS's hotkey
settings - they're global and if you're typing or using your computer while it's on these can
accidentally build up and eat up all your space, etc.

If you saved things to `/dev/shm` as a workaround, remember OBS works in a flatpak so to clean them
up you can use a command like this:

```sh
flatpak enter "$obs_instance" find /dev/shm -maxdepth 1 -type f -name 'Replay*.mp4' -delete
```
