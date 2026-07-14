# macOS code signing & notarization

macOS Gatekeeper blocks plugins that aren't signed with a **Developer ID
Application** certificate and **notarized** by Apple. Without this, users who
download a release get "cannot be opened because the developer cannot be
verified" and have to run `xattr -d com.apple.quarantine ...` by hand.

Packaging is wired to sign, notarize, and staple automatically when credentials
are present, and to **warn and continue with an unsigned package** when they are
not — so `just make-package-dist` works on any machine.
[`obs2/scripts/macos_codesign.sh`](../obs2/scripts/macos_codesign.sh) does the
work after CMake stages the bundle.

## What you need (one-time)

1. An **Apple Developer Program** membership ($99/yr).
2. A **Developer ID Application** certificate. Create it in Xcode
   (_Settings → Accounts → Manage Certificates → + → Developer ID Application_)
   or in the [developer portal](https://developer.apple.com/account/resources/certificates),
   then export it (with its private key) as a password-protected `.p12`.
3. An **App Store Connect API key** for notarization
   ([Users and Access → Integrations → App Store Connect API](https://appstoreconnect.apple.com/access/integrations/api)).
   Download the `.p8` (once only) and note its **Key ID** and the team **Issuer ID**.
   Notarization is an automated scan (seconds to minutes), not human app review.

## Signing locally

Export the following before running `just make-package-dist` (or `just make-package`):

```shell
# Signing identity. Omit to auto-detect a lone "Developer ID Application" cert in
# your login keychain. Value can be the cert's SHA-1 hash or its full name.
export GE_CODESIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"

# Notarization via App Store Connect API key.
export GE_NOTARY_KEY="/path/to/AuthKey_XXXXXXXXXX.p8"
export GE_NOTARY_KEY_ID="XXXXXXXXXX"
export GE_NOTARY_ISSUER="aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"

just make-package-dist
```

Other supported knobs:

- `GE_CODESIGN_KEYCHAIN` — search a specific keychain for the identity.
- `GE_CODESIGN_ENTITLEMENTS` — path to an entitlements plist (not needed by default).
- Notary alternatives to the API key: a `notarytool` keychain profile
  (`GE_NOTARY_KEYCHAIN_PROFILE`) or Apple ID creds
  (`GE_NOTARY_APPLE_ID` + `GE_NOTARY_TEAM_ID` + `GE_NOTARY_PASSWORD`).
- `GE_SKIP_NOTARIZE=1` — sign only, skip notarization/stapling.
- `GE_CODESIGN_REQUIRED=1` — turn "not configured" warnings into hard errors.

Verify the result:

```shell
codesign -dvv --verbose=4 obs2/build/package/*/the_golden_eye.plugin
codesign --verify --strict --deep obs2/build/package/*/the_golden_eye.plugin
xcrun stapler validate obs2/build/package/*/the_golden_eye.plugin
```

The bundle and the inner `libgolden_core.dylib` should both show
`Authority=Developer ID Application` and the `runtime` flag. The dylib is signed
independently so the auto-update core swap keeps a validly-signed core.

## Signing in CI (releases)

Release builds (pushing a `vX.Y.Z` tag) sign and notarize automatically. Ordinary
CI builds stay unsigned to keep them fast. This is driven by
`require-macos-signing: true` in [`release.yml`](../.github/workflows/release.yml),
which makes the packaging step fail loudly if signing can't run.

Add these repository secrets (_Settings → Secrets and variables → Actions_):

| Secret                       | Contents                                              |
| ---------------------------- | ----------------------------------------------------- |
| `MACOS_CERTIFICATE_P12`      | base64 of the `.p12` (`base64 -i cert.p12 \| pbcopy`) |
| `MACOS_CERTIFICATE_PASSWORD` | password protecting the `.p12`                        |
| `MACOS_KEYCHAIN_PASSWORD`    | any string; unlocks the throwaway CI keychain         |
| `MACOS_NOTARY_KEY`           | base64 of the `.p8` API key                           |
| `MACOS_NOTARY_KEY_ID`        | the API key's Key ID                                  |
| `MACOS_NOTARY_ISSUER`        | the App Store Connect Issuer ID                       |

Fork pull requests never receive secrets, so their builds simply produce an
unsigned package without failing.
