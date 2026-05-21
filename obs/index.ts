import fs from "node:fs/promises";

import { OBSWebSocket } from "obs-websocket-js/msgpack";
import blessed, { type Widgets } from "blessed";

import {
  createWelcomeBox,
  createLevelStartBox,
  createLevelFailedBox,
  createLevelCompleteBox,
  createNewPbBox,
  createWaitingBox,
  createLevelInfoBox,
  createRecordingBox,
  createStatisticsBox,
  createWarningBox,
} from "./boxes.ts";
import { basename, dirname, join } from "node:path";
import { LlamaProcess } from "./llama.ts";
import { MatcherProcessPool } from "./matcher.ts";
import { createVideoFileName } from "./naming.ts";
import { type Lang, allLangs, imageWidth, imageHeight } from "./common.ts";
import { ObsError } from "./errors.ts";
import { getMemory, memInfoToString } from "./memory.ts";

const lang: Lang = (() => {
  const envLang = process.env.GE_LANG;
  if (!envLang) {
    return "en";
  }

  if (allLangs.includes(envLang as Lang)) {
    return envLang as Lang;
  }

  throw new Error("Invalid GE_LANG value");
})();

//
// Dependencies
//

const llama = new LlamaProcess();
await llama.initialised;

const matcher = await MatcherProcessPool.init(lang);

const obs = new OBSWebSocket();

const obsConnect = async () => {
  await obs.disconnect().catch(() => {});
  await obs.connect("ws://localhost:4455", process.env.OBS_PASSWORD).catch(ObsError.catch);
};

const moveSavingSome = async (outputPath: string) => {
  const outputDir = dirname(outputPath);
  const failedSaveDir = join(outputDir, "Goldeneye", "failed");
  const failedSavePath = join(failedSaveDir, basename(outputPath, ".mp4"));
  await fs.mkdir(failedSaveDir, { recursive: true });

  // only save a certain number of failed attempts
  const numToSave = 5;
  {
    const entries = await fs.readdir(failedSaveDir);
    const mp4Entries = entries.filter((e) => e.endsWith(".mp4")).sort();
    if (mp4Entries.length >= numToSave) {
      const toDelete = mp4Entries.slice(0, mp4Entries.length - numToSave);
      await Promise.all(toDelete.map((e) => remove(join(failedSaveDir, e))));
    }
  }

  await fs.rename(outputPath, failedSavePath);
};

const remove = async (filepath: string) => {
  await fs.unlink(filepath).catch(() => {});
};

let unhandledError: unknown = null;
const exit = async () => {
  try {
    {
      const { outputActive } = await obs.call("GetRecordStatus");
      if (outputActive) {
        await obs.call("StopRecord");
      }
    }

    await obs.disconnect();
  } finally {
    llama.kill();
    matcher.kill();
    screen.destroy();

    if (unhandledError) {
      console.error("Unhandled error:", unhandledError);
      process.exit(1);
    }

    process.exit();
  }
};

//
// TUI
//

const screen = blessed.screen({
  smartCSR: true,
  title: "The Golden Eye",
});

createWelcomeBox(screen);
const warnBox = blessed.box({
  bottom: 4,
  left: 1,
  width: "shrink",
  height: 1,
  content: "",
  style: {
    fg: "red",
  },
});
const memInfoBox = blessed.box({
  bottom: 3,
  left: 1,
  width: "shrink",
  height: 1,
  content: "",
  style: {
    fg: "white",
    bg: "black",
  },
});
const infoBox = blessed.box({
  bottom: 2,
  left: 1,
  width: "shrink",
  height: 1,
  content: `lang: ${lang}`,
  style: {
    fg: "white",
    bg: "black",
  },
});
const loopTimingBox = blessed.box({
  bottom: 1,
  left: 1,
  width: "shrink",
  height: 1,
  content: "Loop: -- ms",
  style: {
    fg: "white",
    bg: "black",
  },
});

screen.render();

const updateActiveBox = (newBox: Widgets.BoxElement) => {
  screen.children.forEach((child) => screen.remove(child));
  screen.append(newBox);
  screen.append(infoBox);
  screen.append(memInfoBox);
  screen.append(warnBox);

  // if we're still monitoring need to re-append timing box so it stays on top
  if (isMonitoring) {
    screen.append(loopTimingBox);
  } else {
    screen.remove(loopTimingBox);
  }
  screen.render();
};

let pauseToggleRequested = true;
screen.key("space", function () {
  pauseToggleRequested = true;
  onPauseToggleRequested();
});

screen.key(["escape", "q", "C-c"], () => exit());

//
// Main loop
//

let isMonitoring = false;
let inSession = false;
let waitingForStats = false;
let recordingSaveTimer: number | null = null;
let lastMemCheckMs: number = 0;
let memUpdateIntervalMs = 1_000;

let onPauseToggleRequested = () => {};
let saveRecordingResolver: (value: string) => void = () => {};
let saveRecordingPromise: Promise<string> = Promise.reject("nope");
saveRecordingPromise.catch(() => {}); // avoid unhandled rejection if never set

try {
  await obsConnect();

  // if replay buffer is active, stop it, we don't use it and it can cause issues with recording timing
  {
    const result = await obs.call("GetReplayBufferStatus").catch(ObsError.catch);
    if (result?.outputActive) {
      await obs.call("StopReplayBuffer").catch(ObsError.catch);
    }
  }

  for (;;) {
    // pause
    if (pauseToggleRequested) {
      pauseToggleRequested = false;

      inSession = false;
      waitingForStats = false;
      recordingSaveTimer = null;
      isMonitoring = false;
      infoBox.setContent(`Lang: ${lang}`);
      memInfoBox.setContent("");
      updateActiveBox(createWelcomeBox(screen));

      const result = await obs.call("GetRecordStatus").catch(ObsError.catch);
      if (result?.outputActive) {
        await obs.call("StopRecord").then((r) => remove(r.outputPath), ObsError.catch);
      }

      await new Promise<void>((resolve) => (onPauseToggleRequested = resolve));

      isMonitoring = true;
      pauseToggleRequested = false;
      updateActiveBox(createWaitingBox(screen));
    }

    if (Date.now() - lastMemCheckMs > memUpdateIntervalMs) {
      lastMemCheckMs = Date.now();
      const mem = await getMemory();
      memInfoBox.setContent(memInfoToString(mem));
      if (mem.ramPctAvailable < 0.1 || mem.vramPctAvailable < 0.1) {
        memInfoBox.style.fg = "red";
      } else {
        memInfoBox.style.fg = "white";
      }
    }

    await obsConnect();

    // main loop, grab frame
    const start = performance.now();
    const matchResult = await obs
      .call("GetSourceScreenshot", {
        sourceName: process.env.OBS_SOURCE_NAME,
        imageFormat: "jpg",
        imageWidth,
        imageHeight,
      })
      .then((r) => matcher.matchScreen(r.imageData), ObsError.catch);

    if (matchResult) {
      const { screen: gameScreen, matcher } = matchResult;
      infoBox.setContent(`Lang: ${lang}, Detected screen: ${gameScreen} (${matcher})`);

      // if we've been waiting for stats and it's been a few seconds, save the recording and show stats
      if (recordingSaveTimer !== null && (gameScreen !== "EndLevelStats" || Date.now() > recordingSaveTimer + 5000)) {
        const response = await obs.call("GetRecordStatus").catch(ObsError.catch);
        if (response?.outputActive) {
          await obs.call("StopRecord").then((r) => saveRecordingResolver?.(r.outputPath), ObsError.catch);
          updateActiveBox(createWaitingBox(screen));
        } else {
          updateActiveBox(createWarningBox(screen, "expected to be recording but wasn't?"));
        }

        recordingSaveTimer = null;
      }

      // start level
      if (gameScreen === "StartLevel" && !inSession) {
        inSession = true;
        updateActiveBox(createLevelStartBox(screen));

        const response = await obs.call("GetRecordStatus").catch(ObsError.catch);
        if (response?.outputActive === false) {
          await obs.call("StartRecord").catch(ObsError.catch);

          while ((await obs.call("GetRecordStatus").then((r) => r.outputActive, ObsError.catch)) === false) {
            // wait for recording to actually start, note that if OBS has the replay buffer
            // active this can lag by a number of seconds! (hence why we disable it on startup)
            await new Promise((resolve) => setTimeout(resolve, 200));
          }

          updateActiveBox(createRecordingBox(screen));
        } else {
          updateActiveBox(createWarningBox(screen, "already recording when not expected to be recording"));
        }
      }

      // exit to level select
      if (gameScreen === "LevelSelect" && inSession) {
        inSession = false;
        updateActiveBox(createWaitingBox(screen));
        const response = await obs.call("GetRecordStatus").catch(ObsError.catch);
        if (response?.outputActive) {
          await obs.call("StopRecord").then((r) => remove(r.outputPath), ObsError.catch);
        }
      }

      // fail
      if (gameScreen === "EndLevelFailed" && inSession) {
        inSession = false;
        updateActiveBox(createLevelFailedBox(screen));
        const response = await obs.call("GetRecordStatus");
        if (response?.outputActive) {
          await obs.call("StopRecord").then((r) => moveSavingSome(r.outputPath), ObsError.catch);
        }
      }

      // complete
      if (gameScreen === "EndLevelComplete" && inSession) {
        waitingForStats = true;
        inSession = false;
        updateActiveBox(createLevelCompleteBox(screen));
      }

      // stats screen
      if (gameScreen === "EndLevelStats" && waitingForStats) {
        waitingForStats = false;
        recordingSaveTimer = Date.now();
        updateActiveBox(createStatisticsBox(screen));

        // OCR works better with higher quality images, so we fetch a PNG
        const response = await obs
          .call("GetSourceScreenshot", {
            sourceName: process.env.OBS_SOURCE_NAME,
            imageFormat: "png",
            imageWidth,
            imageHeight,
          })
          .catch(ObsError.catch);

        saveRecordingPromise = new Promise<string>((resolve) => {
          saveRecordingResolver = resolve;
        });

        const ocrTimeStart = Date.now();
        llama.sendImage(response!.imageData).then(async ({ levelInfo, llamaResult }) => {
          const isPb = levelInfo.bestTime !== undefined && levelInfo.time < levelInfo.bestTime;

          if (!inSession && isMonitoring && Date.now() < ocrTimeStart + 5000) {
            if (isPb) {
              updateActiveBox(createNewPbBox(screen, levelInfo.time));
            } else {
              updateActiveBox(createLevelInfoBox(screen, levelInfo));
            }
          }

          if (levelInfo.difficulty.toLowerCase().trim() !== llamaResult.difficulty.toLowerCase().trim()) {
            warnBox.setContent(
              `Difficulty mismatch! Parsed: "${levelInfo.difficulty}", OCR: "${llamaResult.difficulty}"`,
            );
            setTimeout(() => warnBox.setContent(""), 10_000);
          }

          const outputPath = await saveRecordingPromise;
          const outputDir = dirname(outputPath);
          const basename = createVideoFileName(levelInfo);

          const recordingPath = join(outputDir, "Goldeneye", `${basename}.mp4`);
          await fs.mkdir(join(outputDir, "Goldeneye"), { recursive: true });
          await fs.rename(outputPath, recordingPath);
        });
      }
    } else {
      infoBox.setContent(`Lang: ${lang}`);
    }

    const end = performance.now();
    const elapsed = end - start;
    loopTimingBox.setContent(`Loop: ${elapsed.toFixed(2)} ms`);
    screen.render();
  }
} catch (err) {
  unhandledError = err;
} finally {
  exit();
}
