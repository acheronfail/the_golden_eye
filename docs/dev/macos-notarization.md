# macOS release signing and notarisation

The tag-triggered release workflow requires signing and notarisation for both macOS architectures.
It signs the packaged core dylib, then the plugin bundle (including the loader), with Developer ID,
a secure timestamp, and hardened runtime. It submits the ZIP to Apple, requires `Accepted`, staples
the ticket to the bundle, and verifies a freshly extracted copy of the final ZIP. Only then can the
workflow upload the package and create a draft release.

The ZIP name and layout stay compatible with the updater. Release checksums are generated from the
final signed and stapled ZIPs. Normal CI and local packages remain unsigned development artifacts;
notarisation credentials are used only by the release packaging step.

## Create a signing certificate

1. Sign in to [Apple Developer](https://developer.apple.com/account) with the paid membership and
   accept any pending agreements. Find the ten-character Team ID in membership details.
2. Open **Keychain Access → Certificate Assistant → Request a Certificate From a Certificate
   Authority…**. Save the certificate signing request to disk.
3. In **Certificates, Identifiers & Profiles → Certificates**, create a **Developer ID Application**
   certificate using that request. Use the current certificate authority offered by Apple. This is
   for distribution outside the Mac App Store; an Apple Development or Mac Distribution certificate
   won't work. A Developer ID Installer certificate is unnecessary for this ZIP distribution.
4. Download and open the certificate to import it into the same Mac's login keychain. In Keychain
   Access, select **My Certificates** and confirm the certificate expands to show its private key.
5. Export that certificate and private key as a password-protected `.p12`. Keep a secure backup of
   the file and its password.
6. Run `security find-identity -v -p codesigning` and copy the full identity, for example
   `Developer ID Application: Your Name (ABCDEFGHIJ)`.

See Apple's
[Developer ID certificate guide](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/).

## Configure GitHub secrets

Under the repository's **Settings → Secrets and variables → Actions**, add these repository secrets:

| Secret                         | Value                                                         |
| ------------------------------ | ------------------------------------------------------------- |
| `MACOS_CERTIFICATE_P12_BASE64` | Base64-encoded exported `.p12`, including its private key.    |
| `MACOS_CERTIFICATE_PASSWORD`   | The nonempty password used when exporting the `.p12`.         |
| `MACOS_SIGNING_IDENTITY`       | The complete `Developer ID Application: … (TEAMID)` identity. |
| `APPLE_ID`                     | The Apple Account email authorised to notarise for this team. |
| `APPLE_TEAM_ID`                | The ten-character Team ID.                                    |
| `APPLE_APP_SPECIFIC_PASSWORD`  | An app-specific password for that Apple Account.              |

On the exporting Mac, copy the encoded certificate directly to the clipboard:

```sh
base64 -i /path/to/DeveloperID.p12 | pbcopy
```

Create an app-specific password at [account.apple.com](https://account.apple.com/) under **Sign-In
and Security → App-Specific Passwords**. Use that password for notarisation, rather than the
account's normal password. Never commit the `.p12`, its password, or notarisation credentials.

The signing script imports the certificate into a temporary keychain, permits noninteractive
`codesign` access, and stores notarisation credentials there. It restores the previous keychain
search list and deletes the temporary keychain on completion or a handled failure. Certificates and
credentials are never uploaded as artifacts. Revoke and replace compromised credentials; renew the
certificate and update the secrets before it expires.

## Build and verify a release

Push a normal release tag after configuring the secrets. Both architecture jobs must complete
signing, notarisation, stapling, and package checks before the draft release is created. Missing
credentials or any signing/notarisation failure blocks the release; there is no unsigned fallback.
Apple submission results and logs are saved as `notarization-macos-<arch>` workflow artifacts. Read
the notary log for warnings as well as errors before publishing.

To run the same process locally, install Xcode command-line tools, export the six secrets above into
your shell environment, and run:

```sh
just make-package-dist
just notarize-macos
just test-package-contract obs2/build
```

Signing changes only the existing ZIP in `obs2/build/dist/`; the staged development bundle stays
unchanged. The original ZIP is replaced only after all validation succeeds. Rebuilding the package
after signing produces an unsigned ZIP again, so sign after the final packaging build.

Before publishing the first signed release, download each architecture's draft asset through a
browser onto a fresh Mac or user account and follow the public installation guide. Keep the
browser's quarantine metadata in place. Confirm OBS loads the plugin without security overrides,
monitor a run, and restart OBS. Then apply a second signed release through the plugin updater and
restart OBS again. The updater replaces the core and resources while retaining the resident loader;
a bundle-wide signature check after a hot update doesn't describe the original release bundle. Test
the actual OBS loading path as well as the initial package's signature and ticket.

Older published ZIPs remain unsigned. The simplified installation guide applies to releases made
with this workflow; publish a successfully verified signed release alongside the updated guide.

See Apple's
[notarisation workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
and
[notarisation requirements](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).
