import assert from "node:assert/strict";
import type { ObsHarness } from "./harness.ts";

export async function playRun(
  h: ObsHarness,
  name: string,
  file: string,
  time: number,
  status: string,
) {
  await h.fixture(name, `clips/${file}`, "ffmpeg_source");
  const eventStart = h.events.length;
  await h.startMonitor(name);
  await h.command({ action: "restart", name });
  await h.expectSavedRun(name, { level: "Runway", timeSeconds: time, status }, eventStart);
  await h.waitFor(
    "media playback ended",
    async () => (await h.command({ action: "status", name })).ended,
  );
  await h.stopMonitor();
  await h.removeSource(name);
  const runs = await h.api("/api/v1/runs");
  assert.equal(runs.clips.filter((clip: any) => clip.metadata.sourceName === name).length, 1);
}
