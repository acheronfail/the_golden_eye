# YouTube OAuth build configuration

The YouTube upload feature ships in every build but stays hidden until enabled at runtime.

## Runtime

- `GE_YOUTUBE_ENABLED`: reveals the `/runs` modal section and `/options` YouTube tab.

## Build

- `GE_YOUTUBE_ENABLED` (build time): set in the environment at CMake configure/build time to bake
  the feature on without the runtime flag

Both are read via `option_env!` and injected by CI from matching Actions secrets (`secrets: inherit`
forwards them from `ci.yml`/`release.yml`). Empty in local builds unless exported.

- `GE_YOUTUBE_CLIENT_SECRET` — required by Google's token endpoint; wrapped with `obfstr`
  (obfuscated to prevent automated scanners, it's not real secrecy).
- `GE_YOUTUBE_CLIENT_ID` — desktop OAuth client ID.

Google/YouTube endpoints are constants in `obs2/rust/src/config/youtube.rs`.

Local dev:

```sh
export GE_YOUTUBE_ENABLED=1
export GE_YOUTUBE_CLIENT_ID='...'
export GE_YOUTUBE_CLIENT_SECRET='...'

just dev
```

## Test hooks (`test-hooks` feature)

Test-only env overrides are named `GE_TEST_YOUTUBE_*` to keep them clearly distinct from the real
`GE_YOUTUBE_*` config. They point the integration at a mock server (`GE_TEST_YOUTUBE_CLIENT_ID`,
`GE_TEST_YOUTUBE_CLIENT_SECRET`, `GE_TEST_YOUTUBE_AUTH_URL`, `GE_TEST_YOUTUBE_TOKEN_URL`,
`GE_TEST_YOUTUBE_UPLOAD_URL`, `GE_TEST_YOUTUBE_USERINFO_URL`, `GE_TEST_YOUTUBE_REDIRECT_URI`,
`GE_TEST_YOUTUBE_OAUTH_STATE`, `GE_TEST_YOUTUBE_TOKEN_FILE`,
`GE_TEST_YOUTUBE_FORCE_KEYRING_FAILURE`) and are gated behind the `test-hooks` cargo feature. The
`test-rust`/`test-integration` recipes enable it; the CMake/package build never does, so shipping
binaries ignore them.
