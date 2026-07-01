# To do...

- Manual verification and user testing
- Good tests for the whole system
- Make sure this all works while streaming
- UI: on kia, death bleed anim?
- Upload: look at `git show fae188a:scripts/upload.ts` for inspiration
    - think about this for the plugin as a whole, since we won't have an approved google oauth app...
- EC: replay buffer enabled check passes when OBS has config that disables use of replay buffer
    - this can happen when replay buffer is actually enabled
    - set "recording quality" to "lossless" and this will disable replay buffer
    - need to check if replay buffer is enabled *and* actually usable
- EC: need to check the max seconds of replay buffer, recommend at east 0x3ff plus some time for cutscenes + configured pads
