import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import * as fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { ObsHarness } from "./harness.ts";
import { playRun } from "./media.ts";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
assert.equal(process.platform, "linux", "Cross-version discovery test requires the OBS Flatpaks");
const builds = path.resolve(process.argv[2]);
const {
  builds: [a, b],
} = JSON.parse(await fs.readFile(path.join(builds, "builds.json"), "utf8"));
const artifacts = await fs.mkdtemp(path.join(root, "obs2/build/real-obs-universal-"));
const legacySource =
  process.env.GE_OBS_TEST_ORIGINAL_U2 ?? path.join(builds, "version-0/the_golden_eye");
const modernSource = path.join(builds, "version-0/the_golden_eye");
const report = {
  passed: false,
  originalUpdater: !!process.env.GE_OBS_TEST_ORIGINAL_U2,
  steps: [] as string[],
};
const hash = async (file: string) =>
  createHash("sha256")
    .update(await fs.readFile(file))
    .digest("hex");
const record = async (step: string) => {
  report.steps.push(step);
  await fs.writeFile(path.join(artifacts, "report.json"), JSON.stringify(report, null, 2));
  console.log("PASS", step);
};
const directory = path.join(artifacts, "transition");
await fs.mkdir(directory);
const options = {
  pluginSource: legacySource,
  releasePackage: path.join(builds, b.package),
  releaseVersion: b.version,
};
let h: ObsHarness | undefined;
const close = async () => {
  const old = h;
  h = undefined;
  await old?.close();
};
const start = async (dir: string, modern: boolean, resume = false, source = legacySource) => {
  h = new ObsHarness(root, dir, true, {
    ...options,
    pluginSource: source,
    modern,
    resume,
    flatpakScope: modern ? "--user" : "--system",
  });
  await h.launch();
  return h;
};
console.log("Universal u2 artifacts:", artifacts);
try {
  await start(directory, false);
  const legacy = h!.pluginDirectory;
  const modern = path.join(directory, "data/obs-studio/plugins/the_golden_eye");
  // A legacy-only fixture exercises provisioning from the update's runtime-data resource.
  await close();
  await fs.rm(modern, { recursive: true, force: true });
  await fs.rm(path.join(legacy, "the_golden_eye.so"), { force: true });
  await fs.rm(path.join(legacy, "data/modern-loader"), { recursive: true, force: true });
  await start(directory, false, true);
  const loaderHash = await hash(path.join(legacy, "bin/64bit/the_golden_eye.so"));
  const update = (await h!.api("/api/v1/updates/check", {})).update;
  assert.equal(update.updaterVersion, 2);
  assert.equal(update.requiresManualInstall, false);
  await h!.api("/api/v1/updates/download", {});
  assert.equal(
    await fs.stat(modern).then(
      () => true,
      () => false,
    ),
    false,
  );
  await h!.reload(() => h!.api("/api/v1/updates/apply", {}), "core reload succeeded");
  assert.equal(
    await hash(path.join(modern, "bin/64bit/libgolden_core.so")),
    await hash(h!.corePath),
  );
  assert.equal(
    await hash(path.join(modern, "the_golden_eye.so")),
    await hash(path.join(legacy, "data/modern-loader/the_golden_eye.so")),
  );
  assert.equal(await hash(path.join(legacy, "bin/64bit/the_golden_eye.so")), loaderHash);
  await record(
    "Direct u2 update provisions the committed modern core and loader without replacing the resident loader",
  );
  await playRun(h!, "Universal legacy replay", "rt4kce-completed.mp4", 28, "complete");
  const runs = await h!.api("/api/v1/runs");
  await close();
  await start(directory, false, true);
  assert.deepEqual(await h!.api("/api/v1/runs"), runs);
  await close();
  await start(directory, true, true);
  assert.deepEqual(await h!.api("/api/v1/runs"), runs);
  const log = await fs.readFile(path.join(directory, "obs.log"), "utf8");
  assert.match(log, /Skipping legacy plugin 'the_golden_eye'.*already loaded/);
  assert.equal((await h!.api("/api/v1/updates/check", {})).update.currentVersion, b.version);
  await playRun(h!, "Universal modern replay", "rt4kce-completed.mp4", 28, "complete");
  await record(
    "OBS32 restart and OBS32 → OBS33 preserve history and save replays; OBS33 skips the retained legacy module",
  );
  const modernRuns = await h!.api("/api/v1/runs");
  await close();
  await start(directory, false, true);
  assert.deepEqual(await h!.api("/api/v1/runs"), modernRuns);
  await h!.fixture("Source after downgrade", "screenshots-emu/en - start - 01 - Secret Agent.png");
  await h!.startMonitor("Source after downgrade");
  await h!.expectMatch("Start", 1, 1, 1);
  await h!.stopMonitor();
  await h!.removeSource("Source after downgrade");
  await close();
  await record("OBS33 → OBS32 preserves run history and gameplay monitoring");
  await fs.rm(legacy, { recursive: true });
  await start(directory, true, true);
  assert.deepEqual(await h!.api("/api/v1/runs"), modernRuns);
  await close();
  await record("Modern-only discovery works without any legacy installation");

  const fresh = path.join(artifacts, "fresh-modern");
  await fs.mkdir(fresh);
  await start(fresh, true, false, modernSource);
  const modernLoaderHash = await hash(path.join(h!.pluginDirectory, "the_golden_eye.so"));
  await h!.api("/api/v1/updates/download", {});
  await h!.reload(() => h!.api("/api/v1/updates/apply", {}), "core reload succeeded");
  assert.equal(
    await hash(h!.corePath),
    await hash(path.join(builds, "version-1/the_golden_eye/bin/64bit/libgolden_core.so")),
  );
  assert.equal(await hash(path.join(h!.pluginDirectory, "the_golden_eye.so")), modernLoaderHash);
  await close();
  await start(fresh, true, true, modernSource);
  assert.equal((await h!.api("/api/v1/updates/check", {})).update.currentVersion, b.version);
  await close();
  await record(
    "Fresh modern installation hot-updates and restarts using the unchanged u2 ZIP contract",
  );

  const denied = path.join(artifacts, "permission-retry");
  const parent = path.join(denied, "data/obs-studio/plugins");
  await fs.mkdir(parent, { recursive: true });
  await fs.chmod(parent, 0o555);
  try {
    await start(denied, false);
    await h!.api("/api/v1/updates/download", {});
    await h!.reload(() => h!.api("/api/v1/updates/apply", {}), "core reload succeeded");
    await h!.waitFor("retained compatibility notification", () =>
      h!.events.find((e) => e.type === "pluginCompatibilityFailed"),
    );
    assert.equal(
      await fs.stat(path.join(parent, "the_golden_eye")).then(
        () => true,
        () => false,
      ),
      false,
    );
    assert.deepEqual(await fs.readdir(parent), []);
    await close();
  } finally {
    await fs.chmod(parent, 0o755);
  }
  await start(denied, false, true);
  await fs.access(path.join(parent, "the_golden_eye/the_golden_eye.so"));
  await close();
  await record(
    "Unwritable modern location leaves the plugin running, notifies late clients, publishes no partial copy, and retries on restart",
  );
  report.passed = true;
} finally {
  try {
    await close();
  } finally {
    await fs.writeFile(path.join(artifacts, "report.json"), JSON.stringify(report, null, 2));
  }
}
