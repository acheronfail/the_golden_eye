# Troubleshooting on Linux

These instructions are for OBS Studio installed through Flatpak.

## Contents

- [Computer slows down while saving replays](#computer-slows-down-while-saving-replays)
  - [Before changing the output path](#before-changing-the-output-path)
  - [Set up RAM-backed replay storage](#set-up-ram-backed-replay-storage)
  - [Check space and clean up](#check-space-and-clean-up)
  - [Return to disk storage](#return-to-disk-storage)

## Computer slows down while saving replays

Saving a long replay buffer can briefly stall other applications, especially when OBS writes to the
same disk as the operating system. OBS saves the entire buffer before the plugin trims the run and
deletes its temporary source. At 10 Mb/s, a 20-minute buffer is roughly 1.5 GB per save; deleting it
afterwards does not undo disk writes already performed.

If the slowdowns coincide with replay saves and you have enough spare RAM, try saving temporary
replays in `/dev/shm`. This is RAM-backed storage, so only the finished clips need to be written to
your normal disk. It will not fix unrelated CPU, GPU, or memory problems.

### Before changing the output path

- **Set the plugin's “Where to save clips?” setting to a permanent folder**, such as a folder in
  your Videos directory. Do not leave it at the default: the default follows OBS's replay output
  folder, which would put finished clips in temporary storage too.
- **This also changes where ordinary OBS recordings and manual replay saves go.** OBS uses its
  Recording Path for both recordings and the replay buffer; there is no built-in setting that
  redirects only this plugin's saves. Copy any wanted recordings out before closing OBS, and restore
  the original path before making normal recordings. A separate OBS profile can help keep these
  settings apart, but recordings in the RAM-backed profile are still temporary.
- **Remove the Save Replay hotkey or use a deliberate key combination** under OBS Settings →
  Hotkeys. A single key such as `S` can repeatedly save the buffer while you type or play. These
  extra saves are not plugin-managed temporary files and can quickly fill RAM. The plugin also
  preserves sources when ownership is ambiguous or trimming fails; do not assume all files will be
  automatically removed.
- **Files here are temporary.** They can disappear when the OBS sandbox ends and are lost on reboot.
  The filesystem's capacity is a limit, not reserved RAM: files consume the same RAM needed by OBS
  and other applications. Leave room for OBS's in-memory replay buffer as well as saved files. On
  systems with swap, tmpfs data can be swapped to disk.

### Set up RAM-backed replay storage

1. Open OBS, then stop the plugin monitor, replay buffer, and any recording before changing paths.
2. Note your original OBS Recording Path. In **Settings → Output → Recording → Recording Path**
   (Advanced output mode), set:

   ```text
   /dev/shm
   ```

   In Simple output mode, use **Settings → Output → Recording → Recording Path** as well. Click
   Apply. Keep the plugin's finished-clip folder on permanent storage.

3. Start monitoring and save a short test run. Confirm the finished clip appears in your permanent
   folder and the plugin's temporary source is removed.

`/dev/shm` already exists in each OBS sandbox, including after restarting OBS. No folder-creation
command, Flatpak permission override, or `sudo` is needed. OBS's sandbox can have a separate
`/dev/shm` from the host, so use the commands below to inspect the files OBS actually sees.

### Check space and clean up

While OBS is running, find its Flatpak instance:

```sh
flatpak ps --columns=instance,application
```

Find the row for `com.obsproject.Studio`. Set the variable below to its numeric instance ID,
replacing the example number, then check space and files in the same terminal:

```sh
obs_instance=1234567890
flatpak enter "$obs_instance" df -h /dev/shm
flatpak enter "$obs_instance" ls -lh /dev/shm
```

Use the instance ID, not the application name. Repeat the lookup after restarting OBS if you need to
inspect or clean up files again; instance IDs change, but the Recording Path stays `/dev/shm`.

If OBS reports “unspecified error while recording”, check **Help → Log Files → View Current Log**.
`No space left on device` means the destination is full; repeated hotkey saves are one possible
cause. A full RAM-backed directory can also leave the rest of the computer short of memory.

Stop the monitor, replay buffer, and recording, and wait for pending clip processing to finish. Copy
any wanted files to permanent storage before cleanup. The following command **permanently deletes
all top-level `Replay*.mp4` files** in this temporary directory, including manual saves:

```sh
flatpak enter "$obs_instance" find /dev/shm -maxdepth 1 -type f -name 'Replay*.mp4' -delete
```

This pattern assumes the default replay prefix and MP4 format. Other names or formats need separate
review. `/dev/shm` also contains shared-memory files used by OBS and its components: never delete
its entire contents. Finished clips in the permanent folder you selected are unaffected.

### Return to disk storage

Stop monitoring and recording, let pending saves finish, and copy out any wanted temporary files.
Restore your original OBS Recording Path (or switch back to your normal recording profile) before
recording anything you want to keep. Keep the plugin's finished-clip folder on disk.
