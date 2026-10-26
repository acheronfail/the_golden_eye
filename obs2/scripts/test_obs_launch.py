import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import run_flatpak
import run_windows


class ObsLaunchTests(unittest.TestCase):
    @unittest.skipIf(os.name == "nt", "Flatpak is Linux-only")
    def test_flatpak_exposes_both_layouts_without_installing_into_user_directories(self):
        for packaged in (False, True):
            with self.subTest(packaged=packaged), tempfile.TemporaryDirectory() as temporary:
                build = Path(temporary).resolve()
                (build / "package/the_golden_eye-linux-x86_64/the_golden_eye").mkdir(parents=True)
                args = ["run_flatpak", temporary, *(["--packaged"] if packaged else [])]
                with patch("sys.argv", args), patch.object(run_flatpak.os, "execvp") as execute:
                    run_flatpak.main()
                command = execute.call_args.args[1]
                plugin = (build / "obs-launch/data/obs-studio/plugins/the_golden_eye").readlink()
                self.assertTrue(str(plugin).startswith(str(build / "package")))
                self.assertIn(f"--env=OBS_PLUGINS_PATH={plugin / 'bin/64bit'}", command)
                self.assertIn(f"XDG_DATA_HOME={build / 'obs-launch/data'}", command)
                self.assertIn(f"--env=OBS_PLUGINS_DATA_PATH={build / 'obs-run-data'}", command)
                core = plugin / "bin/64bit/libgolden_core.so" if packaged else build / "the_golden_eye/bin/64bit/libgolden_core.so"
                self.assertIn(f"--env=GE_CORE_LIB={core}", command)

    def test_windows_uses_explicit_configuration_without_building(self):
        for major in (32, 33):
            for configuration in ("Release", "Debug"):
                with self.subTest(major=major, configuration=configuration), tempfile.TemporaryDirectory() as temporary:
                    build = Path(temporary).resolve()
                    for name in ("Release", "Debug"):
                        (build / name).mkdir()
                        (build / name / "golden_core.dll").write_bytes(b"core")
                    runtime = build / configuration
                    bundle = build / "package/the_golden_eye-windows-x86_64"
                    bundle.mkdir(parents=True)
                    with patch.object(run_windows.subprocess, "run") as stage, patch.object(run_windows.subprocess, "check_output", return_value=str(major)), patch.dict(os.environ, {}, clear=True):
                        env = run_windows.launch_environment(build, configuration)
                    stage.assert_not_called()
                    self.assertEqual(env["OBS_PLUGINS_PATH"], str(bundle if major == 33 else runtime))
                    if major == 33:
                        self.assertEqual(env["GE_CORE_LIB"], str(runtime / "golden_core.dll"))
                    self.assertEqual(env["OBS_PLUGINS_DATA_PATH"], str(build / "obs-run-data"))
                    self.assertEqual(run_windows.runtime_directory(build, configuration), runtime)
                    (runtime / "golden_core.dll").unlink()
                    (build / "golden_core.dll").write_bytes(b"single-config-core")
                    self.assertEqual(run_windows.runtime_directory(build, configuration), build)


if __name__ == "__main__":
    unittest.main()
