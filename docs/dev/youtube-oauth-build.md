# YouTube OAuth build configuration

The YouTube upload feature ships in every build but stays hidden until enabled at runtime.

## Runtime

- `GE_YOUTUBE_ENABLED`: reveals the `/runs` modal section and `/options` YouTube tab.

## Build

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

Env overrides for pointing the integration at a mock server (endpoints, client ID, redirect URI,
`GE_YOUTUBE_TEST_OAUTH_STATE`, `GE_YOUTUBE_TOKEN_FILE`) are gated behind the `test-hooks` cargo
feature. The `test-rust`/`test-integration` recipes enable it; the CMake/package build never does,
so shipping binaries ignore them.
