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
                build = Path(temporary)
                args = ["run_flatpak", temporary, *(["--packaged"] if packaged else [])]
                with patch("sys.argv", args), patch.object(run_flatpak.os, "execvp") as execute:
                    run_flatpak.main()
                command = execute.call_args.args[1]
                plugin = (build / "obs-launch/data/obs-studio/plugins/the_golden_eye").readlink()
                self.assertTrue(str(plugin).startswith(str(build / "package")))
                self.assertIn(f"--env=OBS_PLUGINS_PATH={plugin / 'bin/64bit'}", command)
                self.assertIn(f"XDG_DATA_HOME={build / 'obs-launch/data'}", command)
                data = (build / "obs-launch/legacy-data/the_golden_eye").readlink()
                self.assertEqual(data, plugin / "data" if packaged else build / "obs-run-data/the_golden_eye")
                core = plugin / "bin/64bit/libgolden_core.so" if packaged else build / "the_golden_eye/bin/64bit/libgolden_core.so"
                self.assertIn(f"--env=GE_CORE_LIB={core}", command)

    def test_windows_selects_discovery_from_actual_obs_version_and_keeps_dev_core(self):
        for major in (32, 33):
            with self.subTest(major=major), tempfile.TemporaryDirectory() as temporary:
                build = Path(temporary)
                runtime = build / "Release"
                runtime.mkdir()
                (runtime / "golden_core.dll").write_bytes(b"core")
                bundle = build / "package/the_golden_eye-windows-x86_64"
                bundle.mkdir(parents=True)
                with patch.object(run_windows, "obs_executable", return_value="C:/OBS/obs64.exe"), patch.object(run_windows.subprocess, "run") as stage, patch.object(run_windows.subprocess, "check_output", return_value=str(major)), patch.dict(os.environ, {}, clear=True):
                    env = run_windows.launch_environment(build)
                self.assertEqual(env["OBS_PLUGINS_PATH"], str(bundle if major == 33 else runtime))
                if major == 33:
                    stage.assert_called_once()
                    self.assertEqual(env["GE_CORE_LIB"], str(runtime / "golden_core.dll"))
                else:
                    stage.assert_not_called()
                self.assertEqual(env["OBS_PLUGINS_DATA_PATH"], str(build / "obs-run-data"))


if __name__ == "__main__":
    unittest.main()
