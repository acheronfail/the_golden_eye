import assert from "node:assert/strict";
import * as fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { ObsHarness } from "./harness.ts";
import { runSuite, type Scenario } from "./suite.ts";

const root = await fs.realpath(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../.."));
const args = process.argv.slice(2);
let repeats = 1;
let software = false;
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--repeat") repeats = Number(args[++i]);
  else if (args[i] === "--software-renderer") software = true;
  else throw new Error(`Unknown argument: ${args[i]}`);
}
assert(Number.isInteger(repeats) && repeats > 0 && repeats <= 100, "--repeat must be 1–100");
await fs.mkdir(path.join(root, "obs2/build"), { recursive: true });
const scenarios: Scenario<ObsHarness>[] = [];
const defineScenario = (
  name: string,
  run: (h: ObsHarness) => Promise<void>,
): Scenario<ObsHarness> => ({
  name,
  run,
});
scenarios.push(
  defineScenario("1080p capture, source reconnection, and resolution changes", async (h) => {
    await h.fixture("Image", "screenshots-emu/en - start - 01 - Secret Agent.png");
    await h.startMonitor("Image");
    await h.expectMatch("Start", 1, 1, 1);
    await h.expectCapture(640, 480);
    await h.removeSource("Image");
    assert(h.snapshot.monitor.enabled, "monitor survives source removal");
    await h.fixture("Image", "screenshots-av2hdmi/en - start - 03 - Secret Agent.png");
    await h.expectMatch("Start", 1, 3, 1);
    await h.expectCapture(640, 480);
    await h.stopMonitor();
    await h.removeSource("Image");
  }),
);
scenarios.push(
  defineScenario("Japanese calibration and stretched stats", async (h) => {
    await h.fixture("Japanese", "screenshots-av2hdmi/jp - start - 10 - Secret Agent.png");
    await h.startMonitor("Japanese");
    await h.expectMatch("Start", 6, 1, 1);
    const eventStart = h.events.length;
    await h.fixture(
      "Japanese",
      "screenshots-av2hdmi_16x9/jp - stats - 10 - Secret Agent - 0347_1106_1212.png",
    );
    await h.expectMatch("Stats", 6, 1, 1);
    assert.deepEqual(h.snapshot.match.raw_times, [227, 666, 732]);
    await h.expectCapture(640, 480);
    await h.expectSavedRun(
      "Japanese",
      { level: "Statue", difficulty: "Secret Agent", timeSeconds: 227, status: "complete" },
      eventStart,
    );
    await h.stopMonitor();
    await h.removeSource("Japanese");
  }),
);
for (const scenario of [
  {
    name: "Completed",
    file: "rt4kce-completed.mp4",
    status: "complete",
    time: 28,
    level: "Runway",
  },
  { name: "KIA", file: "kia.mp4", status: "kia", time: 14, level: "Runway" },
]) {
  scenarios.push(
    defineScenario(`${scenario.name} media playback and real replay save`, async (h) => {
      await h.fixture(scenario.name, `clips/${scenario.file}`, "ffmpeg_source");
      const eventStart = h.events.length;
      await h.startMonitor(scenario.name);
      await h.command({ action: "restart", name: scenario.name });
      await h.expectSavedRun(
        scenario.name,
        { level: scenario.level, timeSeconds: scenario.time, status: scenario.status },
        eventStart,
      );
      await h.waitFor(
        "media playback ended",
        async () => (await h.command({ action: "status", name: scenario.name })).ended,
      );
      await h.stopMonitor();
      await h.removeSource(scenario.name);
      const runs = await h.api("/api/v1/runs");
      assert.equal(
        runs.clips.filter((clip: any) => clip.metadata.sourceName === scenario.name).length,
        1,
      );
    }),
  );
}

const artifacts = await fs.mkdtemp(path.join(root, "obs2/build/real-obs-"));
let interrupted = false;
let current: ObsHarness | undefined;
for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, () => {
    interrupted = true;
    current?.abort();
    process.exitCode = 1;
  });
}
console.log(`Suite artifacts: ${artifacts}`);
const report = await runSuite({
  artifacts,
  scenarios,
  repeats,
  interrupted: () => interrupted,
  create: (directory) => {
    current = new ObsHarness(root, directory, software);
    return current;
  },
});
if (report.failed || report.skipped || interrupted) process.exitCode = 1;
