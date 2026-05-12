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
} from './tui/boxes';
import { fileURLToPath } from 'node:url';
import { LevelInfo } from './parse';
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

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const exit = async () => {
  try {
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
        await fs.unlink(outputPath);
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

    const gameScreen = await matchScreen(imageData);
    if (gameScreen) {
      if (recordingSaveTimer !== null && (gameScreen !== 'EndLevelStats' || Date.now() > recordingSaveTimer + 5000)) {
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          saveRecordingResolver?.(outputPath);
        } else {
          // TODO: warning (so can use replay buffer as backup)
        }

        recordingSaveTimer = null;
        updateActiveBox(createWaitingBox(screen));
      }

      if (gameScreen === 'StartLevel' && !inLevel) {
        inLevel = true;
        updateActiveBox(createLevelStartBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (!outputActive) {
          await obs.call('StartRecord');
        } else {
          // TODO: warning recording is already started
        }
        updateActiveBox(createRecordingBox(screen));
      }

      if (gameScreen === 'LevelSelect' && inLevel) { // exit to level select
        inLevel = false;
        updateActiveBox(createWaitingBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          await fs.unlink(outputPath);
        }
      }

      if (gameScreen === 'EndLevel' && inLevel) { // fail
        inLevel = false;
        updateActiveBox(createLevelFailedBox(screen));
        const { outputActive } = await obs.call('GetRecordStatus');
        if (outputActive) {
          const { outputPath } = await obs.call('StopRecord');
          await fs.unlink(outputPath);
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

            await fs.rename(outputPath, join(outputDir, basename));
            // TODO: folder sort + YT upload
          }
        });
      }
    }

    const end = performance.now();
    const elapsed = end - start;
    loopTimingBox.setContent(`Loop: ${elapsed.toFixed(2)} ms ${screen.children.length}`);
    screen.render();
  }
} finally {
  exit();
}
