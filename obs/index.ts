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
  await fs.unlink(filepath).catch(() => {});
};

const exit = async () => {
  try {
    {
      const { outputActive } = await obs.call('GetRecordStatus');
      if (outputActive) {
        await obs.call('StopRecord');
      }
    }

    {
      const { outputActive } = await obs.call('GetReplayBufferStatus');
      if (outputActive) {
        await obs.call('StopReplayBuffer');
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

let onPauseToggleRequested = () => {};
let saveRecordingResolver: (value: string) => void = () => {};
let saveRecordingPromise: Promise<string> = Promise.reject('nope');
saveRecordingPromise.catch(() => {}); // avoid unhandled rejection if never set

try {
  await obs.connect('ws://localhost:4455', process.env.OBS_PASSWORD);

  // start replay buffer, this is used if there are any error cases as a backup
  // source of recording
  {
    const { outputActive } = await obs.call('GetReplayBufferStatus');
    if (!outputActive) {
      await obs.call('StartReplayBuffer');
    }
  }

  for (;;) {
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

    const start = performance.now();
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });

    const matchResult = await matcher.matchScreen(imageData);
    if (matchResult) {
      const { screen: gameScreen } = matchResult;
      if (recordingSaveTimer !== null && (gameScreen !== 'EndLevelStats' || Date.now() > recordingSaveTimer + 5000)) {
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          saveRecordingResolver?.(outputPath);
          updateActiveBox(createWaitingBox(screen));
        } else {
          await obs.call('SaveReplayBuffer');
          updateActiveBox(createWarningBox(screen, "expected to be recording but wasn't, saved replay buffer instead"));
        }

        recordingSaveTimer = null;
      }

      if (gameScreen === 'StartLevel' && !inSession) {
        inSession = true;
        updateActiveBox(createLevelStartBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (!outputActive) {
          await obs.call('StartRecord');
          updateActiveBox(createRecordingBox(screen));
        } else {
          await obs.call('SaveReplayBuffer');
          updateActiveBox(
            createWarningBox(
              screen,
              'already recording when not expected to be recording, saved replay buffer instead',
            ),
          );
        }
      }

      if (gameScreen === 'LevelSelect' && inSession) {
        // exit to level select
        inSession = false;
        updateActiveBox(createWaitingBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          await remove(outputPath);
        }
      }

      if (gameScreen === 'EndLevelFailed' && inSession) {
        // fail
        inSession = false;
        updateActiveBox(createLevelFailedBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          await remove(outputPath);
        }
      }

      if (gameScreen === 'EndLevelComplete' && inSession) {
        waitingForStats = true;
        inSession = false;
        updateActiveBox(createLevelCompleteBox(screen));
      }

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
