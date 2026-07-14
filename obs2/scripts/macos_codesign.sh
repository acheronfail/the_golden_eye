#!/usr/bin/env bash

# Signs, notarizes, and staples the macOS package produced by `package-plugin`.
# Degrades to a warning (never fails) when a signing identity or notary credentials
# are absent, so `just make-package-dist` works without an Apple setup. Set
# GE_CODESIGN_REQUIRED=1 to turn those warnings into hard errors (CI releases).
# Runs after CMake staged the bundle and wrote the unsigned dist zip.

set -euo pipefail
shopt -s nullglob

LOG_PREFIX="[macos-codesign]"
BUNDLE_NAME="the_golden_eye.plugin"
CORE_DYLIB="libgolden_core.dylib"

log() { echo "$LOG_PREFIX $*"; }

# Missing config -> warn and exit 0, unless GE_CODESIGN_REQUIRED forces a failure.
warn_or_fail() {
  if [ "${GE_CODESIGN_REQUIRED:-}" = "1" ]; then
    echo "$LOG_PREFIX error: $*" >&2
    exit 1
  fi
  echo "$LOG_PREFIX warning: $*" >&2
  exit 0
}

rezip() {
  local bundle="$1" zip="$2"
  mkdir -p "$(dirname "$zip")"
  rm -f "$zip"
  ditto -c -k --keepParent "$bundle" "$zip"
}

build_dir="${1:-obs2/build}"

bundles=("$build_dir"/package/*/"$BUNDLE_NAME")
[ ${#bundles[@]} -gt 0 ] || warn_or_fail "no staged $BUNDLE_NAME under $build_dir/package (run package-plugin first)"
bundle="${bundles[${#bundles[@]} - 1]}"

zips=("$build_dir"/dist/the_golden_eye-macos-*.zip)
[ ${#zips[@]} -gt 0 ] || warn_or_fail "no dist zip under $build_dir/dist (run package-plugin first)"
zip="${zips[${#zips[@]} - 1]}"

# Resolve the signing identity: explicit env, else a lone "Developer ID
# Application" cert. Its hash (field 2 of `find-identity`) is what codesign wants.
identity="${GE_CODESIGN_IDENTITY:-}"
if [ -z "$identity" ]; then
  matches="$(security find-identity -v -p codesigning 2>/dev/null | grep "Developer ID Application" || true)"
  if [ -z "$matches" ]; then
    warn_or_fail "no Developer ID Application signing identity found; leaving the package unsigned. See docs/signing-macos.md."
  fi
  if [ "$(printf '%s\n' "$matches" | wc -l | tr -d ' ')" -gt 1 ]; then
    warn_or_fail "multiple Developer ID Application identities found; set GE_CODESIGN_IDENTITY to pick one"
  fi
  identity="$(printf '%s\n' "$matches" | awk '{print $2}')"
  log "auto-detected signing identity: $identity"
fi

sign_flags=(--force --options runtime --timestamp --sign "$identity")
[ -n "${GE_CODESIGN_KEYCHAIN:-}" ] && sign_flags+=(--keychain "$GE_CODESIGN_KEYCHAIN")
[ -n "${GE_CODESIGN_ENTITLEMENTS:-}" ] && sign_flags+=(--entitlements "$GE_CODESIGN_ENTITLEMENTS")

# Sign the swappable core dylib before the bundle so its own Developer ID
# signature travels with it through the auto-update core swap.
core="$bundle/Contents/MacOS/$CORE_DYLIB"
if [ -f "$core" ]; then
  log "signing $CORE_DYLIB"
  codesign "${sign_flags[@]}" "$core"
fi
log "signing $BUNDLE_NAME"
codesign "${sign_flags[@]}" "$bundle"
codesign --verify --strict --verbose=2 "$bundle"
log "signature verified"

rezip "$bundle" "$zip"

if [ "${GE_SKIP_NOTARIZE:-}" = "1" ]; then
  log "GE_SKIP_NOTARIZE=1 set; result: signed (not notarized)"
  exit 0
fi

creds=()
if [ -n "${GE_NOTARY_KEY:-}" ] && [ -n "${GE_NOTARY_KEY_ID:-}" ] && [ -n "${GE_NOTARY_ISSUER:-}" ]; then
  creds=(--key "$GE_NOTARY_KEY" --key-id "$GE_NOTARY_KEY_ID" --issuer "$GE_NOTARY_ISSUER")
elif [ -n "${GE_NOTARY_KEYCHAIN_PROFILE:-}" ]; then
  creds=(--keychain-profile "$GE_NOTARY_KEYCHAIN_PROFILE")
elif [ -n "${GE_NOTARY_APPLE_ID:-}" ] && [ -n "${GE_NOTARY_TEAM_ID:-}" ] && [ -n "${GE_NOTARY_PASSWORD:-}" ]; then
  creds=(--apple-id "$GE_NOTARY_APPLE_ID" --team-id "$GE_NOTARY_TEAM_ID" --password "$GE_NOTARY_PASSWORD")
else
  warn_or_fail "no notary credentials found; the package is signed but not notarized. See docs/signing-macos.md."
fi

log "submitting to Apple notary service (this can take a few minutes)..."
output="$(xcrun notarytool submit "$zip" "${creds[@]}" --wait 2>&1)" && rc=0 || rc=$?
printf '%s\n' "$output"
if [ "$rc" -ne 0 ] || ! printf '%s\n' "$output" | grep -q "status: Accepted"; then
  id="$(printf '%s\n' "$output" | grep -Eo 'id: [0-9a-fA-F-]{36}' | head -1 | awk '{print $2}')"
  [ -n "$id" ] && xcrun notarytool log "$id" "${creds[@]}" || true
  warn_or_fail "notarization did not succeed"
fi

log "stapling ticket to $BUNDLE_NAME"
xcrun stapler staple "$bundle"
rezip "$bundle" "$zip"
log "result: signed+notarized"
