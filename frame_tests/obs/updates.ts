import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import * as fs from "node:fs/promises";
import path from "node:path";
import { playRun } from "./media.ts";
import type { ObsHarness } from "./harness.ts";
import type { Scenario } from "./suite.ts";

const digest = async (file: string) =>
  createHash("sha256")
    .update(await fs.readFile(file))
    .digest("hex");
const absent = async (file: string) => {
  await assert.rejects(fs.access(file), { code: "ENOENT" });
};

async function stage(h: ObsHarness, mode: "valid" | "rollback") {
  await h.updateMode(mode);
  await h.api("/api/v1/updates/download", {});
  assert.equal((await h.api("/api/v1/updates/status")).phase, "staged");
  await fs.access(path.join(h.stagedDirectory, path.basename(h.corePath)));
}

export const rollbackScenario: Scenario<ObsHarness> = {
  name: "Failed core startup rolls back inside the same OBS process",
  async run(h) {
    const before = await digest(h.corePath);
    const runs = await h.api("/api/v1/runs");
    const eventStart = h.events.length;
    const templateHash = await digest(path.join(h.dataDirectory, "cv_templates/en-colon.png"));
    await h.fixture("Update rollback", "screenshots-emu/en - start - 01 - Secret Agent.png");
    await stage(h, "rollback");
    const settings = (await h.api("/api/v1/settings/status")).settings;
    await h.reload(() => h.api("/api/v1/updates/apply", {}), "rolled back to the running version");
    assert.equal(await digest(h.corePath), before);
    await absent(h.stagedDirectory);
    await absent(path.join(h.dataDirectory, "obs-update-test.txt"));
    assert.deepEqual(await h.api("/api/v1/runs"), runs);
    assert.deepEqual((await h.api("/api/v1/settings/status")).settings, settings);
    assert.equal(
      await digest(path.join(h.dataDirectory, "cv_templates/en-colon.png")),
      templateHash,
    );
    assert(h.snapshot.sources.some((source: any) => source.name === "Update rollback"));
    await h.startMonitor("Update rollback");
    await h.expectMatch("Start", 1, 1, 1);
    await h.stopMonitor();
    await h.removeSource("Update rollback");
    await playRun(h, "Rollback replay", "rt4kce-completed.mp4", 28, "complete");
    assert(
      !h.events.slice(eventStart).some((event) => event.type === "updateApplied"),
      "rollback must not announce a successful update",
    );
  },
};

export const updateScenarios: Scenario<ObsHarness>[] = [
  {
    name: "Update checksum rejection preserves the running plugin",
    async run(h) {
      const before = await digest(h.corePath);
      await h.updateMode("checksum");
      await h.api("/api/v1/updates/download", {}, "POST", 500);
      await absent(path.join(h.stagedDirectory, path.basename(h.corePath)));
      assert.equal(await digest(h.corePath), before);
      assert.match(
        await fs.readFile(path.join(h.artifacts, "obs.log"), "utf8"),
        /checksum verification/,
      );
      await h.fixture("Update checksum", "screenshots-emu/en - start - 01 - Secret Agent.png");
      await h.startMonitor("Update checksum");
      await h.expectMatch("Start", 1, 1, 1);
      await h.stopMonitor();
      await h.removeSource("Update checksum");
    },
  },
  rollbackScenario,
  {
    name: "Automatic update waits for monitoring to stop and reloads core and data",
    async run(h) {
      const runs = await h.api("/api/v1/runs");
      await h.updateMode("valid");
      await h.fixture("Update deferred", "screenshots-emu/en - start - 01 - Secret Agent.png");
      await h.startMonitor("Update deferred");
      await h.expectMatch("Start", 1, 1, 1);
      const settings = (await h.api("/api/v1/settings/status")).settings;
      await h.api("/api/v1/settings", { ...settings, autoUpdateEnabled: true }, "PUT");
      await h.api("/api/v1/updates/check", {});
      await h.waitFor(
        "automatic update staged",
        async () => (await h.api("/api/v1/updates/status")).phase === "staged",
        30000,
      );
      await h.api("/api/v1/updates/apply", {}, "POST", 409);
      assert(h.snapshot.monitor.enabled);
      await absent(path.join(h.dataDirectory, "obs-update-test.txt"));
      await h.expectMatch("Start", 1, 1, 1);
      const eventStart = h.events.length;
      await h.reload(() => h.api("/api/v1/monitor/stop", {}), "core reload succeeded");
      await h.waitFor("update applied notice", () =>
        h.events.slice(eventStart).some((event) => event.type === "updateApplied"),
      );
      assert.equal(
        await fs.readFile(path.join(h.dataDirectory, "obs-update-test.txt"), "utf8"),
        "valid",
      );
      await absent(h.stagedDirectory);
      assert.deepEqual(await h.api("/api/v1/runs"), runs);
      assert.equal(h.snapshot.settingsStatus.settings.autoUpdateEnabled, true);
      await h.api("/api/v1/settings", settings, "PUT");
      await h.startMonitor("Update deferred");
      await h.expectMatch("Start", 1, 1, 1);
      await h.stopMonitor();
      await h.removeSource("Update deferred");
    },
  },
];
