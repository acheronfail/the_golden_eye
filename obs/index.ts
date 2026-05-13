import fs from 'node:fs/promises';

import { OBSWebSocket } from 'obs-websocket-js/msgpack';
import blessed, { type Widgets } from 'blessed';

import { readEnv } from './envfile.ts';
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
} from './boxes.ts';
import { dirname, join } from 'node:path';
import { LlamaProcess } from './llama.ts';
import { MatcherProcessPool } from './matcher.ts';
import { createVideoFileName } from './naming.ts';

await readEnv();

//
// Dependencies
//

const llama = new LlamaProcess();
await llama.initialised;

const matcher = await MatcherProcessPool.init();

const obs = new OBSWebSocket();

const remove = async (filepath: string) => {
  await fs.unlink(filepath).catch(() => { });
};

const exit = async () => {
  try {
    {
      const { outputActive } = await obs.call('GetRecordStatus');
      if (outputActive) {
        await obs.call('StopRecord');
      }
    }

    await obs.disconnect();
  } finally {
    llama.kill();
    process.exit();
  }
};

//
// State
//

let isMonitoring = false;
let inSession = false;
let waitingForStats = false;
let recordingSaveTimer: number | null = null;

//
// TUI
//

const screen = blessed.screen({
  smartCSR: true,
  title: 'The Golden Eye',
});

createWelcomeBox(screen);
const loopTimingBox = blessed.box({
  bottom: 1,
  left: 1,
  width: 'shrink',
  height: 1,
  content: 'Loop: -- ms',
  style: {
    fg: 'white',
    bg: 'black',
  },
});

screen.render();

const updateActiveBox = (newBox: Widgets.BoxElement) => {
  screen.children.forEach((child) => screen.remove(child));
  screen.append(newBox);

  // if we're still monitoring need to re-append timing box so it stays on top
  screen.append(loopTimingBox);
  if (isMonitoring) {
    screen.append(loopTimingBox);
  } else {
    screen.remove(loopTimingBox);
  }
  screen.render();
};

let pauseToggleRequested = true;
screen.key('space', function () {
  pauseToggleRequested = true;
  onPauseToggleRequested();
});

screen.key(['escape', 'q', 'C-c'], () => exit());

//
// Main loop
//

let onPauseToggleRequested = () => { };
let saveRecordingResolver: (value: string) => void = () => { };
let saveRecordingPromise: Promise<string> = Promise.reject('nope');
saveRecordingPromise.catch(() => { }); // avoid unhandled rejection if never set

try {
  await obs.connect('ws://localhost:4455', process.env.OBS_PASSWORD);

  // if replay buffer is active, stop it, we don't use it and it can cause issues with recording timing
  {
    const { outputActive } = await obs.call('GetReplayBufferStatus');
    if (outputActive) {
      await obs.call('StopReplayBuffer');
    }
  }

  for (; ;) {
    // pause
    if (pauseToggleRequested) {
      pauseToggleRequested = false;

      inSession = false;
      waitingForStats = false;
      recordingSaveTimer = null;
      isMonitoring = false;
      updateActiveBox(createWelcomeBox(screen));

      const { outputActive } = await obs.call('GetRecordStatus');
      if (outputActive) {
        const { outputPath } = await obs.call('StopRecord');
        await remove(outputPath);
      }

      await new Promise<void>((resolve) => (onPauseToggleRequested = resolve));

      isMonitoring = true;
      pauseToggleRequested = false;
      updateActiveBox(createWaitingBox(screen));
    }

    // main loop, grab frame
    const start = performance.now();
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });

    const matchResult = await matcher.matchScreen(imageData);
    if (matchResult) {
      const { screen: gameScreen } = matchResult;

      // if we've been waiting for stats and it's been a few seconds, save the recording and show stats
      if (recordingSaveTimer !== null && (gameScreen !== 'EndLevelStats' || Date.now() > recordingSaveTimer + 5000)) {
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          saveRecordingResolver?.(outputPath);
          updateActiveBox(createWaitingBox(screen));
        } else {
          updateActiveBox(createWarningBox(screen, "expected to be recording but wasn't?"));
        }

        recordingSaveTimer = null;
      }

      // start level
      if (gameScreen === 'StartLevel' && !inSession) {
        inSession = true;
        updateActiveBox(createLevelStartBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (!outputActive) {
          await obs.call('StartRecord');
          while (!(await obs.call('GetRecordStatus')).outputActive) {
            // wait for recording to actually start, note that if OBS has the reply buffer
            // active this can lag by a number of seconds!
            await new Promise((resolve) => setTimeout(resolve, 200));
          }
          updateActiveBox(createRecordingBox(screen));
        } else {
          updateActiveBox(
            createWarningBox(
              screen,
              'already recording when not expected to be recording',
            ),
          );
        }
      }

      // exit to level select
      if (gameScreen === 'LevelSelect' && inSession) {
        inSession = false;
        updateActiveBox(createWaitingBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          await remove(outputPath);
        }
      }

      // fail
      if (gameScreen === 'EndLevelFailed' && inSession) {
        inSession = false;
        updateActiveBox(createLevelFailedBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          await remove(outputPath);
        }
      }

      // complete
      if (gameScreen === 'EndLevelComplete' && inSession) {
        waitingForStats = true;
        inSession = false;
        updateActiveBox(createLevelCompleteBox(screen));
      }

      // stats screen
      if (gameScreen === 'EndLevelStats' && waitingForStats) {
        waitingForStats = false;
        recordingSaveTimer = Date.now();
        updateActiveBox(createStatisticsBox(screen));

        // OCR works better with higher quality images, so we fetch a PNG
        const { imageData } = await obs.call('GetSourceScreenshot', {
          sourceName: process.env.SOURCE_NAME,
          imageFormat: 'png',
        });

        saveRecordingPromise = new Promise<string>((resolve) => {
          saveRecordingResolver = resolve;
        });

        const ocrTimeStart = Date.now();
        llama.sendImage(imageData).then(async (levelInfo) => {
          const isPb = levelInfo.bestTime !== undefined && levelInfo.time < levelInfo.bestTime;

          if (!inSession && isMonitoring && Date.now() < ocrTimeStart + 5000) {
            if (isPb) {
              updateActiveBox(createNewPbBox(screen, levelInfo.time));
            } else {
              updateActiveBox(createLevelInfoBox(screen, levelInfo));
            }
          }

          const outputPath = await saveRecordingPromise;
          const outputDir = dirname(outputPath);
          const basename = createVideoFileName(levelInfo);

          const recordingPath = join(outputDir, 'Goldeneye', `${basename}.mp4`);
          await fs.mkdir(join(outputDir, 'Goldeneye'), { recursive: true });
          await fs.rename(outputPath, recordingPath);
        });
      }
    }

    const end = performance.now();
    const elapsed = end - start;
    loopTimingBox.setContent(`Loop: ${elapsed.toFixed(2)} ms`);
    screen.render();
  }
} finally {
  exit();
}
