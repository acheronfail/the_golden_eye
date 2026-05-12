import cp from 'node:child_process';

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
} from './tui/boxes';
import { fileURLToPath } from 'node:url';
import { LevelInfo } from './parse';

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

const exit = async () => {
  await obs.disconnect();
  llamaProc.kill();
  process.exit();
};

//
// State
//

let isMonitoring = false;
let inLevel = false;
let waitingForStats = false;

let recordingBoxTimer: NodeJS.Timeout | null = null;

//
// TUI
//

const screen = blessed.screen({
  smartCSR: true,
  title: 'The Golden Eye',
});

let activeBox = createWelcomeBox(screen);
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

screen.append(loopTimingBox);
screen.render();

const updateActiveBox = (newBox: Widgets.BoxElement) => {
  recordingBoxTimer && clearTimeout(recordingBoxTimer);
  screen.remove(activeBox);
  activeBox = newBox;
  screen.append(activeBox);
  screen.render();
};

screen.key('space', function () {
  inLevel = false;
  waitingForStats = false;
  isMonitoring = !isMonitoring;
  updateActiveBox(isMonitoring ? createWaitingBox(screen) : createWelcomeBox(screen));
});

screen.key(['escape', 'q', 'C-c'], () => exit());

//
// Main loop
//

try {
  await obs.connect('ws://localhost:4455', process.env.OBS_PASSWORD);

  for (;;) {
    const start = performance.now();
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });

    const gameScreen = await matchScreen(imageData);
    if (gameScreen) {
      if (gameScreen === 'StartLevel' && !inLevel) {
        inLevel = true;
        updateActiveBox(createLevelStartBox(screen));
        recordingBoxTimer = setTimeout(() => updateActiveBox(createLevelStartBox(screen)), 1000);
      }

      if (gameScreen === 'EndLevel' && inLevel) {
        inLevel = false;
        updateActiveBox(createLevelFailedBox(screen));
      }

      if (gameScreen === 'EndLevelComplete' && inLevel) {
        waitingForStats = true;
        inLevel = false;
        updateActiveBox(createLevelCompleteBox(screen));
      }

      if (gameScreen === 'EndLevelStats' && waitingForStats) {
        waitingForStats = false;
        llamaProc.send({ type: 'extract-level-info', imageData });
        llamaProc.once('message', (message: any) => {
          if (message.type === 'level-info' && !inLevel && !waitingForStats) {
            const info = message as LevelInfo;
            if (info.time < info.bestTime) {
              updateActiveBox(createNewPbBox(screen, info.time));
            } else {
              updateActiveBox(createLevelInfoBox(screen, info));
            }
          }
        });
      }
    }

    const end = performance.now();
    const elapsed = end - start;
    loopTimingBox.setContent(`Loop: ${elapsed.toFixed(1)} ms`);
  }
} finally {
  exit();
}
