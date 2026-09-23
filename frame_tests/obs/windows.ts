import assert from "node:assert/strict";
import * as fs from "node:fs/promises";
import path from "node:path";

export async function findWindowsObs() {
  const candidates = process.env.GE_OBS_TEST_INSTALLATION
    ? [process.env.GE_OBS_TEST_INSTALLATION]
    : [
        path.join(process.env.ProgramFiles ?? "C:/Program Files", "obs-studio"),
        path.join(
          process.env.SCOOP ?? path.join(process.env.USERPROFILE!, "scoop"),
          "apps/obs-studio/current",
        ),
      ];
  for (const candidate of candidates) {
    try {
      await fs.access(path.join(candidate, "bin/64bit/obs64.exe"));
      return await fs.realpath(candidate);
    } catch (error: any) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  throw new Error(
    "OBS was not found. Set GE_OBS_TEST_INSTALLATION to its installation directory (containing bin/64bit/obs64.exe).",
  );
}

export async function copyWindowsObs(source: string, destination: string) {
  const target = path.join(
    await fs.realpath(path.dirname(destination)),
    path.basename(destination),
  );
  const relative = path.relative(await fs.realpath(source), target);
  assert(
    relative.startsWith(".." + path.sep) || path.isAbsolute(relative),
    "Test OBS copy must be outside the installed OBS directory",
  );
  // Copy runtime files only: installed profiles and plugin copies must not enter the test.
  for (const directory of ["bin", "data", "obs-plugins"]) {
    await fs.cp(path.join(source, directory), path.join(destination, directory), {
      recursive: true,
      dereference: true,
      filter: (file) =>
        !["the_golden_eye", "the_golden_eye.dll", "golden_core.dll"].includes(
          path.basename(file).toLowerCase(),
        ),
    });
  }
}

export async function readWindowsObsLog(config: string) {
  const directory = path.join(config, "obs-studio/logs");
  let files: string[];
  try {
    files = (await fs.readdir(directory)).filter((name) => name.endsWith(".txt")).sort();
  } catch (error: any) {
    if (error.code === "ENOENT") return "";
    throw error;
  }
  return files.length ? fs.readFile(path.join(directory, files.at(-1)!), "utf8") : "";
}
