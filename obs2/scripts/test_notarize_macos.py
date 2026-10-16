from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import notarize_macos as signing


class ReleaseSigningTests(unittest.TestCase):
    def setUp(self):
        self.credentials = {name: "secret-value" for name in signing.REQUIRED_SECRETS}
        self.credentials["MACOS_SIGNING_IDENTITY"] = "Developer ID Application: Test (ABCDEFGHIJ)"
        self.environment = patch.dict(os.environ, self.credentials, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def test_missing_credentials_fail_closed(self):
        del os.environ["APPLE_TEAM_ID"]
        with self.assertRaisesRegex(RuntimeError, "Missing release secrets: APPLE_TEAM_ID"):
            signing.validate_credentials()

    def test_development_identity_is_rejected(self):
        os.environ["MACOS_SIGNING_IDENTITY"] = "Apple Development: Test"
        with self.assertRaisesRegex(RuntimeError, "Developer ID Application"):
            signing.validate_credentials()

    def test_command_failure_does_not_expose_secrets(self):
        result = subprocess.CompletedProcess([], 1, stdout="")
        with patch.object(signing.subprocess, "run", return_value=result):
            with self.assertRaises(RuntimeError) as error:
                signing.run("security", "import", "sensitive-password")
        self.assertNotIn("sensitive-password", str(error.exception))

    def exercise_submission(self, status, fail_at=None):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        package = root / "release.zip"
        package.write_bytes(b"original unsigned package")
        work = root / "work"
        work.mkdir()
        calls = []

        def command(*args, **kwargs):
            calls.append(args)
            if fail_at and fail_at(args):
                raise RuntimeError("injected validation failure")
            if args[:3] == ("xcrun", "notarytool", "submit"):
                return json.dumps({"id": "submission-id", "status": status})
            if args[:3] == ("ditto", "-c", "-k"):
                Path(args[-1]).write_bytes(b"signed and stapled package")
            return ""

        return root, package, work, calls, command

    def test_rejection_keeps_original_and_fetches_log(self):
        root, package, work, calls, command = self.exercise_submission("Invalid")
        with patch.object(signing, "run", side_effect=command):
            with self.assertRaisesRegex(RuntimeError, "not accepted: Invalid"):
                signing.notarize(package, root / "logs", work, root / "keychain")
        self.assertEqual(package.read_bytes(), b"original unsigned package")
        self.assertTrue((root / "logs/submission.json").is_file())
        self.assertTrue(any(call[:3] == ("xcrun", "notarytool", "log") for call in calls))
        self.assertFalse(any(call[:2] == ("xcrun", "stapler") for call in calls))

    def test_acceptance_verifies_repacked_zip_before_replacement(self):
        root, package, work, calls, command = self.exercise_submission("Accepted")
        with patch.object(signing, "run", side_effect=command):
            signing.notarize(package, root / "logs", work, root / "keychain")
        self.assertEqual(package.read_bytes(), b"signed and stapled package")
        self.assertEqual(calls[-1], ("xcrun", "stapler", "validate", work / "verification" / signing.PLUGIN))
        signs = [call for call in calls if call[:2] == ("codesign", "--force")]
        self.assertEqual(signs[0][-1], work / signing.PLUGIN / "Contents/MacOS/libgolden_core.dylib")
        self.assertEqual(signs[1][-1], work / signing.PLUGIN)
        self.assertTrue(all("--timestamp" in call and "runtime" in call for call in signs))

    def test_final_ticket_failure_keeps_original(self):
        root, package, work, _, command = self.exercise_submission(
            "Accepted", lambda args: args[:3] == ("xcrun", "stapler", "validate") and "verification" in str(args[-1]))
        with patch.object(signing, "run", side_effect=command):
            with self.assertRaisesRegex(RuntimeError, "injected validation failure"):
                signing.notarize(package, root / "logs", work, root / "keychain")
        self.assertEqual(package.read_bytes(), b"original unsigned package")

    def test_keychain_is_restored_and_deleted_on_failure(self):
        os.environ["MACOS_CERTIFICATE_P12_BASE64"] = "dGVz\ndA==\n"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(signing, "run", return_value='"/original/login.keychain-db"') as command:
                with self.assertRaisesRegex(RuntimeError, "signing failure"):
                    with signing.signing_keychain(root):
                        raise RuntimeError("signing failure")
            self.assertEqual(command.call_args_list[-2].args,
                ("security", "list-keychains", "-d", "user", "-s", "/original/login.keychain-db"))
            self.assertEqual(command.call_args_list[-1].args,
                ("security", "delete-keychain", root / "signing.keychain-db"))
            self.assertFalse((root / "certificate.p12").exists())


if __name__ == "__main__":
    unittest.main()
