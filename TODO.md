# To do...

- Manual verification and user testing
- Good tests for the whole system
- Make sure this all works while streaming
- UI: consider a "runs" page which lists all runs in the save directory, and lets you view them in the browser
    - future iterations should allow you to upload them to youtube as well
    - Upload: look at `git show fae188a:scripts/upload.ts` for inspiration
- BG: linux has linking errors for libglu:
    - `error: [the-golden-eye] failed to dlopen core: libGLU.so.1: cannot open shared object file: No such file or directory\nerror: [the-golden-eye] core failed to load; plugin disabled\nwarning: Failed to initialize module 'the_golden_eye.so'`
    - should we dump the log somewhere from the thin plugin? or show some kind of alert?
- BG: when folders don't exist for clip saving, we need to create them
