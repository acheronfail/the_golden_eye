import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import * as fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { ObsHarness, type HarnessOptions } from "./harness.ts";
import { playRun } from "./media.ts";
import { runSuite } from "./suite.ts";

const root = await fs.realpath(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../.."));
const builds = path.resolve(process.argv[2]);
assert(
  process.argv.slice(3).every((arg) => arg === "--software-renderer"),
  "Unknown argument",
);
const software = process.argv.includes("--software-renderer");
const manifest = JSON.parse(await fs.readFile(path.join(builds, "builds.json"), "utf8"));
assert.equal(manifest.builds.length, 2);
const [a, b] = manifest.builds;
const pluginName = process.platform === "darwin" ? "the_golden_eye.plugin" : "the_golden_eye";
const coreRelative =
  process.platform === "darwin"
    ? "Contents/MacOS/libgolden_core.dylib"
    : "bin/64bit/libgolden_core.so";
const loaderRelative =
  process.platform === "darwin" ? "Contents/MacOS/the_golden_eye" : "bin/64bit/the_golden_eye.so";
const hash = async (file: string) =>
  createHash("sha256")
    .update(await fs.readFile(file))
    .digest("hex");
for (const build of manifest.builds)
  assert.equal(
    await hash(path.join(builds, build.package)),
    build.sha256,
    "Package matches build manifest",
  );
const hashA = await hash(path.join(builds, "version-0", pluginName, coreRelative));
const hashB = await hash(path.join(builds, "version-1", pluginName, coreRelative));
assert.notEqual(hashA, hashB, "A and B are distinct compiled cores");
const options: HarnessOptions = {
  pluginSource: path.join(builds, "version-0", pluginName),
  releasePackage: path.join(builds, b.package),
  releaseVersion: b.version,
};

class UpgradeSession {
  h?: ObsHarness;
  readonly directory: string;
  constructor(directory: string) {
    this.directory = directory;
  }
  async launch() {
    this.h = new ObsHarness(root, this.directory, software, options);
    await this.h.launch();
  }
  async close() {
    const h = this.h;
    this.h = undefined;
    await h?.close();
  }
  async restart() {
    await this.close();
    this.h = new ObsHarness(root, this.directory, software, { ...options, resume: true });
    await this.h.launch();
  }
}

let current: UpgradeSession | undefined;
let interrupted = false;
for (const signal of ["SIGINT", "SIGTERM"] as const)
  process.on(signal, () => {
    interrupted = true;
    current?.h?.abort();
    process.exitCode = 1;
  });
const artifacts = await fs.mkdtemp(path.join(root, "obs2/build/real-obs-upgrade-"));
await fs.writeFile(path.join(artifacts, "builds.json"), JSON.stringify(manifest, null, 2));
const report = await runSuite({
  artifacts,
  repeats: 1,
  interrupted: () => interrupted,
  create: (directory) => (current = new UpgradeSession(directory)),
  scenarios: [
    {
      name: `Upgrade ${a.version} to ${b.version} and restart OBS`,
      async run(session) {
        let h = session.h!;
        assert.equal(await hash(h.corePath), hashA);
        const loaderHash = await hash(path.join(h.pluginDirectory, loaderRelative));
        const check = await h.api("/api/v1/updates/check", {});
        assert.equal(check.update.currentVersion, a.version);
        assert.equal(check.update.latestVersion, `v${b.version}`);
        assert.equal(check.update.requiresManualInstall, false);
        const settings = (await h.api("/api/v1/settings/status")).settings;
        await h.api("/api/v1/settings", { ...settings, preRunPaddingSecs: 2 }, "PUT");
        await playRun(h, "Before upgrade", "rt4kce-completed.mp4", 28, "complete");
        const runsBefore = await h.api("/api/v1/runs");
        const eventStart = h.events.length;
        await h.reload(async () => {
          await h.api(
            "/api/v1/settings",
            { ...settings, preRunPaddingSecs: 2, autoUpdateEnabled: true },
            "PUT",
          );
          await h.api("/api/v1/updates/check", {});
        }, "core reload succeeded");
        const notice = await h.waitFor("version B update notice", () =>
          h.events.slice(eventStart).find((event) => event.type === "updateApplied"),
        );
        assert.equal(notice.version, b.version);
        assert.equal(
          notice.releaseUrl,
          "https://github.com/acheronfail/the_golden_eye/releases/tag/simulated",
        );
        assert.equal(await hash(h.corePath), hashB);
        assert.equal(
          await hash(path.join(h.pluginDirectory, loaderRelative)),
          loaderHash,
          "Resident loader stays unchanged",
        );
        assert.deepEqual(await h.api("/api/v1/runs"), runsBefore);
        assert.equal(h.snapshot.settingsStatus.settings.preRunPaddingSecs, 2);
        assert.equal((await h.api("/api/v1/updates/check", {})).update, null);
        await playRun(h, "After upgrade", "kia.mp4", 14, "kia");
        const runsAfter = await h.api("/api/v1/runs");
        const buildIdB = h.events
          .slice(eventStart)
          .find((event) => event.type === "version")!.buildId;
        await h.ready();
        await session.restart();
        h = session.h!;
        assert.equal(await hash(h.corePath), hashB, "B remains installed after OBS restarts");
        assert.equal(h.events.find((event) => event.type === "version")!.buildId, buildIdB);
        assert.equal(
          (await h.api("/api/v1/updates/check", {})).update,
          null,
          "Restarted core is up to date with B",
        );
        assert.deepEqual(await h.api("/api/v1/runs"), runsAfter);
        assert.equal(h.snapshot.settingsStatus.settings.preRunPaddingSecs, 2);
        await h.fixture("After restart", "screenshots-emu/en - start - 01 - Secret Agent.png");
        await h.startMonitor("After restart");
        await h.expectMatch("Start", 1, 1, 1);
        await h.stopMonitor();
        await h.removeSource("After restart");
        await h.ready();
      },
    },
  ],
});
if (report.failed || report.skipped || interrupted) process.exitCode = 1;
