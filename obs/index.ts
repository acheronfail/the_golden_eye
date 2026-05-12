import cp from 'node:child_process';
import { OBSWebSocket } from 'obs-websocket-js/msgpack';

import { readEnv } from './envfile';
import { matchScreen } from './matcher';
import { fileURLToPath } from 'node:url';

await readEnv();

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

let inLevel = false;
let waitingForStats = false;

try {
  await obs.connect('ws://localhost:4455', process.env.OBS_PASSWORD);

  console.log('Starting main loop!');
  for (;;) {
    const start = performance.now();
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });

    const screen = await matchScreen(imageData);
    if (screen) {
      if (process.env.DEBUG) {
        console.log(`Matched screen: ${screen}`);
      }

      if (screen === 'StartLevel' && !inLevel) {
        inLevel = true;
        console.log('Started level!');
      }

      if (screen === 'EndLevel' && inLevel) {
        inLevel = false;
        console.log('Ended level!');
      }

      if (screen === 'EndLevelComplete' && inLevel) {
        waitingForStats = true;
        inLevel = false;
        console.log('Waiting for stats screen...');
      }

      if (screen === 'EndLevelStats' && waitingForStats) {
        waitingForStats = false;
        console.log('Extracting level info...');
        llamaProc.send({ type: 'extract-level-info', imageData });
      }
    }

    if (process.env.DEBUG) {
      const end = performance.now();
      process.stderr.write(`\rloop time: ${end - start}ms` + ' '.repeat(20));
      process.stderr.cursorTo(0);
    }
  }
} finally {
  await obs.disconnect();
  llamaProc.kill();
  process.exit();
}
