#!/usr/bin/env python3

from __future__ import annotations

import argparse
import base64
import contextlib
import json
import os
import secrets
import shlex
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path


REQUIRED_SECRETS = (
    "MACOS_CERTIFICATE_P12_BASE64",
    "MACOS_CERTIFICATE_PASSWORD",
    "MACOS_SIGNING_IDENTITY",
    "APPLE_ID",
    "APPLE_TEAM_ID",
    "APPLE_APP_SPECIFIC_PASSWORD",
)
PLUGIN = "the_golden_eye.plugin"


def run(*args: str | Path, capture: bool = False, allow_failure: bool = False) -> str:
    result = subprocess.run([str(arg) for arg in args], text=True, stdout=subprocess.PIPE if capture else None)
    if result.returncode and not allow_failure:
        # Commands can contain credentials; never include their arguments in errors.
        raise RuntimeError(f"{args[0]} failed with exit code {result.returncode}")
    return result.stdout if capture else ""


def validate_credentials() -> None:
    missing = [name for name in REQUIRED_SECRETS if not os.environ.get(name)]
    if missing:
        raise RuntimeError(f"Missing release secrets: {', '.join(missing)}; see docs/dev/macos-notarization.md")
    if not os.environ["MACOS_SIGNING_IDENTITY"].startswith("Developer ID Application: "):
        raise RuntimeError("MACOS_SIGNING_IDENTITY must be a Developer ID Application identity")


@contextlib.contextmanager
def signing_keychain(temporary: Path):
    keychain = temporary / "signing.keychain-db"
    certificate = temporary / "certificate.p12"
    encoded = "".join(os.environ["MACOS_CERTIFICATE_P12_BASE64"].split())
    certificate.write_bytes(base64.b64decode(encoded, validate=True))
    certificate.chmod(0o600)
    password = secrets.token_urlsafe(32)
    previous = shlex.split(run("security", "list-keychains", "-d", "user", capture=True))
    run("security", "create-keychain", "-p", password, keychain)
    try:
        run("security", "set-keychain-settings", "-lut", "21600", keychain)
        run("security", "unlock-keychain", "-p", password, keychain)
        run("security", "list-keychains", "-d", "user", "-s", *previous, keychain)
        run("security", "import", certificate, "-k", keychain, "-P",
            os.environ["MACOS_CERTIFICATE_PASSWORD"], "-T", "/usr/bin/codesign")
        run("security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:",
            "-s", "-k", password, keychain, capture=True)
        certificate.unlink()
        yield keychain
    finally:
        try:
            run("security", "list-keychains", "-d", "user", "-s", *previous)
        finally:
            run("security", "delete-keychain", keychain)


def archive_bundle(bundle: Path, destination: Path) -> None:
    run("ditto", "-c", "-k", "--keepParent", "--norsrc", bundle, destination)


def verify_bundle(bundle: Path) -> None:
    run("codesign", "--verify", "--deep", "--strict", "--verbose=2", bundle)
    run("codesign", "--verify", "--strict", "--verbose=2", bundle / "Contents/MacOS/libgolden_core.dylib")


def notarize(package: Path, diagnostics: Path, temporary: Path, keychain: Path) -> None:
    bundle = temporary / PLUGIN
    run("ditto", "-x", "-k", package, temporary)
    identity = os.environ["MACOS_SIGNING_IDENTITY"]
    signing_args = ("--force", "--sign", identity, "--keychain", keychain, "--timestamp", "--options", "runtime")
    # Sign nested code first; signing the bundle also signs its resident loader.
    run("codesign", *signing_args, bundle / "Contents/MacOS/libgolden_core.dylib")
    run("codesign", *signing_args, bundle)
    verify_bundle(bundle)

    run("xcrun", "notarytool", "store-credentials", "golden-eye", "--keychain", keychain,
        "--apple-id", os.environ["APPLE_ID"], "--team-id", os.environ["APPLE_TEAM_ID"],
        "--password", os.environ["APPLE_APP_SPECIFIC_PASSWORD"])
    auth = ("--keychain-profile", "golden-eye", "--keychain", keychain)
    submission = temporary / "submission.zip"
    archive_bundle(bundle, submission)
    response = run("xcrun", "notarytool", "submit", submission, *auth,
        "--wait", "--timeout", "30m", "--output-format", "json", capture=True, allow_failure=True)
    diagnostics.mkdir(parents=True, exist_ok=True)
    (diagnostics / "submission.json").write_text(response)
    result = json.loads(response)
    if result.get("id"):
        run("xcrun", "notarytool", "log", result["id"], *auth, diagnostics / "log.json")
    if result.get("status") != "Accepted":
        raise RuntimeError(f"Apple notarisation was not accepted: {result.get('status')}; see {diagnostics}")

    run("xcrun", "stapler", "staple", bundle)
    run("xcrun", "stapler", "validate", bundle)
    verify_bundle(bundle)
    final = temporary / "notarized.zip"
    archive_bundle(bundle, final)
    # Verify the actual distributable after a fresh extraction, including its ticket.
    extracted = temporary / "verification"
    run("ditto", "-x", "-k", final, extracted)
    verify_bundle(extracted / PLUGIN)
    run("xcrun", "stapler", "validate", extracted / PLUGIN)
    os.replace(final, package)
    print(f"Signed, notarised, and stapled: {package.name}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Sign and notarise the existing macOS release ZIP in place.")
    parser.add_argument("build_dir", type=Path)
    args = parser.parse_args()
    if sys.platform != "darwin":
        raise RuntimeError("macOS signing requires macOS and Xcode command-line tools")
    validate_credentials()
    build = args.build_dir.resolve()
    packages = list((build / "dist").glob("the_golden_eye-*-macos-*.zip"))
    if len(packages) != 1:
        raise RuntimeError("Expected exactly one macOS release ZIP in the build's dist directory")
    with zipfile.ZipFile(packages[0]) as archive:
        if any(Path(name).is_absolute() or ".." in Path(name).parts or Path(name).parts[0] != PLUGIN
               for name in archive.namelist()):
            raise RuntimeError(f"Release ZIP must contain only {PLUGIN}")
    # Keep temporary output on the same volume for atomic ZIP replacement.
    with tempfile.TemporaryDirectory(prefix="ge-notarize-", dir=build) as directory:
        temporary = Path(directory)
        with signing_keychain(temporary) as keychain:
            notarize(packages[0], build / "notarization", temporary, keychain)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, ValueError, OSError) as error:
        print(f"macOS release failed: {error}", file=sys.stderr)
        raise SystemExit(1)
