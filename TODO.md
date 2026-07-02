# To do...

- Manual verification and user testing
- Good tests for the whole system
- Make sure this all works while streaming
- UI: consider a "runs" page which lists all runs in the save directory, and lets you view them in the browser
    - future iterations should allow you to upload them to youtube as well
    - Upload: look at `git show fae188a:scripts/upload.ts` for inspiration
- Core: should we dump errors or the log somewhere from the thin plugin? or show some kind of alert?
- BG: when folders don't exist for clip saving, we need to create them
