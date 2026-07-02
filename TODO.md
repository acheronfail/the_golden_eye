# To do...

- Manual verification and user testing
- Good tests for the whole system
- Make sure this all works while streaming
- UI: consider a "runs" page which lists all runs in the save directory, and lets you view them in the browser
    - future iterations should allow you to upload them to youtube as well
    - Upload: look at `git show fae188a:scripts/upload.ts` for inspiration
- Core: should we dump errors or the log somewhere from the thin plugin? or show some kind of alert?
- Core: `error: os_dlopen(/home/acheronfail/src/ge-obs/obs2/build/libgolden_core.so->/home/acheronfail/src/ge-obs/obs2/build/libgolden_core.so): /usr/lib/x86_64-linux-gnu/libm.so.6: version 'GLIBC_2.43' not found (required by /home/acheronfail/src/ge-obs/obs2/build/libgolden_core.so)\nwarning: Module '/home/acheronfail/src/ge-obs/obs2/build/libgolden_core.so' not loaded`
- BG: when folders don't exist for clip saving, we need to create them
