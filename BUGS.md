# Bugs

### VRAM leak

It's been observed on Linux systems that OBS has a VRAM leak. Reproduce by doing the following:

1. Check VRAM (i.e., `nvidia-smi`)
2. Start the program `just run`
3. Press `space` to start the monitor
4. Quit `q`
5. Check VRAM again 

There don't seem to be any resources not cleaned up by exiting the program, but the VRAM stays used.
If OBS is restarted then the VRAM is released back to the system.
