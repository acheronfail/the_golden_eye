# Troubleshooting on Linux

These instructions are for OBS Studio installed through Flatpak.

## Contents

- [Computer slows down while saving replays](#computer-slows-down-while-saving-replays)
  - [Before changing the output path](#before-changing-the-output-path)
  - [Set up RAM-backed replay storage](#set-up-ram-backed-replay-storage)
  - [Check space and clean up](#check-space-and-clean-up)
  - [Return to disk storage](#return-to-disk-storage)

## Computer slows down while saving replays

Saving a long replay buffer can briefly stall other applications, especially when OBS writes to
the same disk as the operating system. OBS saves the entire buffer before the plugin trims the
run and deletes its temporary source. At 10 Mb/s, a 20-minute buffer is roughly 1.5 GB per save;
deleting it afterwards does not undo disk writes already performed.

If the slowdowns coincide with replay saves and you have enough spare RAM, try saving temporary
replays in `/dev/shm/golden-eye-replays`. This is RAM-backed storage, so only the finished clips
need to be written to your normal disk. It will not fix unrelated CPU, GPU, or memory problems.

### Before changing the output path

- **Set the plugin's “Where to save clips?” setting to a permanent folder**, such as a folder in
  your Videos directory. Do not leave it at the default: the default follows OBS's replay output
  folder, which would put finished clips in temporary storage too.
- **This also changes where ordinary OBS recordings and manual replay saves go.** OBS uses its
  Recording Path for both recordings and the replay buffer; there is no built-in setting that
  redirects only this plugin's saves. Copy any wanted recordings out before closing OBS, and
  restore the original path before making normal recordings. A separate OBS profile can help
  keep these settings apart, but recordings in the RAM-backed profile are still temporary.
- **Remove the Save Replay hotkey or use a deliberate key combination** under OBS Settings →
  Hotkeys. A single key such as `S` can repeatedly save the buffer while you type or play. These
  extra saves are not plugin-managed temporary files and can quickly fill RAM. The plugin also
  preserves sources when ownership is ambiguous or trimming fails; do not assume all files
  will be automatically removed.
- **Files here are temporary.** They can disappear when the OBS sandbox ends and are lost on
  reboot. The filesystem's capacity is a limit, not reserved RAM: files consume the same RAM
  needed by OBS and other applications. Leave room for OBS's in-memory replay buffer as well
  as saved files. On systems with swap, tmpfs data can be swapped to disk.

### Set up RAM-backed replay storage

1. Open OBS, then stop the plugin monitor, replay buffer, and any recording before changing paths.
2. In a terminal, find OBS's running Flatpak instance:

   ```sh
   flatpak ps --columns=instance,application
   ```

   Find the row for `com.obsproject.Studio`. Set the variable below to its numeric instance ID,
   replacing the example number. Run the following commands in the same terminal:

   ```sh
   obs_instance=1234567890
   flatpak enter "$obs_instance" mkdir -p -m 700 /dev/shm/golden-eye-replays
   flatpak enter "$obs_instance" df -h /dev/shm/golden-eye-replays
   ```

   Use the instance ID, not the application name. OBS's Flatpak can have a separate `/dev/shm`
   from the host: creating the folder with a plain host `mkdir` may leave OBS unable to see it.
   The commands above create and inspect the folder inside the running OBS sandbox.
3. Note your original OBS Recording Path. In **Settings → Output → Recording → Recording Path**
   (Advanced output mode), set:

   ```text
   /dev/shm/golden-eye-replays
   ```

   In Simple output mode, use **Settings → Output → Recording → Recording Path** as well.
   Click Apply. Keep the plugin's finished-clip folder on permanent storage.
4. Start monitoring and save a short test run. Confirm the finished clip appears in your permanent
   folder and the plugin's temporary source is removed.

Repeat the instance lookup and folder-creation commands after restarting OBS. Instance IDs change,
and the folder may no longer exist. “Bad File Path” usually means the folder was not created in
the current OBS sandbox. No Flatpak permission override or `sudo` should be needed.

### Check space and clean up

With the current `obs_instance` set as above:

```sh
flatpak enter "$obs_instance" df -h /dev/shm/golden-eye-replays
flatpak enter "$obs_instance" ls -lh /dev/shm/golden-eye-replays
```

If OBS reports “unspecified error while recording”, check **Help → Log Files → View Current Log**.
`No space left on device` means the destination is full; repeated hotkey saves are one possible
cause. A full RAM-backed directory can also leave the rest of the computer short of memory.

Stop the monitor, replay buffer, and recording, and wait for pending clip processing to finish.
Copy any wanted files to permanent storage before cleanup. The following command **permanently
deletes all top-level `Replay*.mp4` files** in this temporary directory, including manual saves:

```sh
flatpak enter "$obs_instance" find /dev/shm/golden-eye-replays -maxdepth 1 -type f -name 'Replay*.mp4' -delete
```

This pattern assumes the default replay prefix and MP4 format. Other names or formats need
separate review; do not replace it with a command that blindly deletes the whole directory.
Finished clips in the permanent folder you selected are unaffected.

### Return to disk storage

Stop monitoring and recording, let pending saves finish, and copy out any wanted temporary files.
Restore your original OBS Recording Path (or switch back to your normal recording profile)
before recording anything you want to keep. Keep the plugin's finished-clip folder on disk.
