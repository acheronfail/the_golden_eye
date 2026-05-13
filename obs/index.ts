import cp from 'node:child_process';
import fs from 'node:fs/promises';

import { OBSWebSocket } from 'obs-websocket-js/msgpack';
import blessed, { Widgets } from 'blessed';

import { readEnv } from './envfile';
import { matchScreen } from './matcher';
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
} from './boxes';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

await readEnv();

//
// Dependencies
//

const llamaProc = cp.fork(fileURLToPath(new URL('./llama-process.js', import.meta.url)), [], {
  serialization: 'advanced',
  stdio: 'inherit',
});
await new Promise((resolve) =>
  llamaProc.once('message', (message: any) => {
    if (message.type === 'ready') {
      resolve(null);
    }
  }),
);

const obs = new OBSWebSocket();

const remove = async (filepath: string) => {
  await fs.unlink(filepath).catch(() => {});
}

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
    llamaProc.kill();
    process.exit();
  }
};


//
// State
//

let isMonitoring = false;
let inLevel = false;
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
let saveRecordingResolver: ((value: string) => void) = () => { };
let saveRecordingPromise: Promise<string> = Promise.reject('nope');
saveRecordingPromise.catch(() => { }); // avoid unhandled rejection if never set

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

  for (; ;) {
    if (pauseToggleRequested) {
      pauseToggleRequested = false;

      inLevel = false;
      waitingForStats = false;
      recordingSaveTimer = null;
      isMonitoring = false;
      updateActiveBox(createWelcomeBox(screen));

      const { outputActive } = await obs.call('GetRecordStatus');
      if (outputActive) {
        const { outputPath } = await obs.call('StopRecord')
        await remove(outputPath);
      }

      await new Promise<void>(resolve => onPauseToggleRequested = resolve);

      isMonitoring = true;
      pauseToggleRequested = false;
      updateActiveBox(createWaitingBox(screen));
    }

    const start = performance.now();
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });

    const matchResult = await matchScreen(imageData);
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

      if (gameScreen === 'StartLevel' && !inLevel) {
        inLevel = true;
        updateActiveBox(createLevelStartBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (!outputActive) {
          await obs.call('StartRecord');
          updateActiveBox(createRecordingBox(screen));
        } else {
          await obs.call('SaveReplayBuffer');
          updateActiveBox(createWarningBox(screen, "already recording when not expected to be recording, saved replay buffer instead"));
        }
      }

      if (gameScreen === 'LevelSelect' && inLevel) { // exit to level select
        inLevel = false;
        updateActiveBox(createWaitingBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          await remove(outputPath);
        }
      }

      if (gameScreen === 'EndLevelFailed' && inLevel) { // fail
        inLevel = false;
        updateActiveBox(createLevelFailedBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          await remove(outputPath);
        }
      }

      if (gameScreen === 'EndLevelComplete' && inLevel) {
        waitingForStats = true;
        inLevel = false;
        updateActiveBox(createLevelCompleteBox(screen));
      }

      if (gameScreen === 'EndLevelStats' && waitingForStats) {
        waitingForStats = false;
        recordingSaveTimer = Date.now();

        const { imageData } = await obs.call('GetSourceScreenshot', {
          sourceName: process.env.SOURCE_NAME,
          imageFormat: 'png',
        });

        updateActiveBox(createStatisticsBox(screen));


        saveRecordingPromise = new Promise<string>(resolve => {
          saveRecordingResolver = resolve;
        });

        llamaProc.send({ type: 'extract-level-info', imageData });
        llamaProc.once('message', async (message: any) => {
          if (message.type === 'level-info') {
            const { levelInfo } = message;
            const isPb = levelInfo.bestTime !== undefined && levelInfo.time < levelInfo.bestTime;

            if (!inLevel && isMonitoring) {
              if (isPb) {
                updateActiveBox(createNewPbBox(screen, levelInfo.time));
              } else {
                updateActiveBox(createLevelInfoBox(screen, levelInfo));
              }
            }

            const outputPath = await saveRecordingPromise;
            const outputDir = dirname(outputPath);
            const formattedTime = `${Math.floor(levelInfo.time / 60).toString().padStart(2, '0')}-${(levelInfo.time % 60).toString().padStart(2, '0')}`;

            const basename = [
              levelInfo.levelNumber.toString().padStart(2, '0'),
              levelInfo.level,
              levelInfo.difficulty,
              formattedTime,
              new Date().toISOString(),
            ].join(' - ');

            await fs.rename(outputPath, join(outputDir, `${basename}.mp4`));
            // TODO: place in directory and then YT upload
          }
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
