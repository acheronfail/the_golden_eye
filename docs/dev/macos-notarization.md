# macOS release signing

Tag-triggered releases sign and notarize both macOS architectures using
`acheronfail/dist-tools/actions/apple-release@v1` (plugin support requires v1.1.0+).
The signing plan lists the core dylib first; signing the outer plugin then seals
the resident loader and resources. The Action uses Developer ID Application,
hardened runtime and timestamps, and staples Apple's accepted ticket.

The workflow preserves the updater's ZIP names and layout. Before upload it
packages the signed plugin without resource-fork metadata, checks the package
contract, and verifies both signatures and the stapled ticket after extraction. Release checksums use those final ZIPs. Normal CI and
local packages remain development artifacts.

## Credentials

Use the [dist-tools credential CLI](https://github.com/acheronfail/dist-tools/blob/v1/docs/credentials.md)
with `acheronfail/the_golden_eye` in its repository allowlist. For team API-key
authentication, sync these settings:

| Setting | Type |
| --- | --- |
| `MACOS_CODE_SIGN_P12_BASE64` | Secret |
| `MACOS_CODE_SIGN_P12_PASSWORD` | Secret |
| `APPLE_TEAM_ID` | Secret |
| `APPLE_NOTARY_API_KEY` | Secret |
| `APPLE_NOTARY_KEY_ID` | Secret |
| `APPLE_NOTARY_ISSUER_ID` | Secret |
| `APPLE_NOTARY_AUTH_MODE` = `api-team` | Variable |

Apple Account compatibility uses `APPLE_NOTARY_APPLE_ID` and
`APPLE_NOTARY_PASSWORD` with mode `apple-id` (the default).

## Verification

Local packaging remains available through `just make-package-dist` and
`just test-package-contract obs2/build`. Distribution signing runs in hosted CI;
there is no local certificate import or notarization script in this repository.

Before publishing a signed release, install a quarantined download of each
architecture on a fresh Mac and confirm OBS loads it. Test the updater with a
second signed release too: it replaces the core and resources while retaining
the resident loader, so testing only the initial bundle is insufficient.
