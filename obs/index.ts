import { OBSWebSocket } from 'obs-websocket-js/msgpack';

import { Llama } from './llama';
import { extractLevelInfo } from './parse';
import { readEnv } from './envfile';
import { matchScreen, Screen } from './matcher';

await readEnv();

const llama = new Llama();
await llama.initialised;

const obs = new OBSWebSocket();

let inLevel = false;
let waitingForStats = false;

// TODO: upload to YT, something like https://github.com/jakzo/NeonWhiteMods/blob/main/scripts/upload-to-youtube.ts
try {
  await obs.connect('ws://localhost:4455', process.env.OBS_PASSWORD);

  for (;;) {
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });

    const screen = await matchScreen(imageData);
    if (screen) {
      if (process.env.DEBUG) {
        console.log(`Matched screen: ${screen}`);
      }

      if (screen === 'StartLevel') {
        inLevel = true;
      }

      if (screen === 'EndLevel') {
        inLevel = false;
      }

      if (screen === 'EndLevelComplete' && inLevel) {
        waitingForStats = true;
        inLevel = false;
      }

      if (screen === 'EndLevelStats' && waitingForStats) {
        waitingForStats = false;

        // TODO: do this on a background worker/thread
        // const text = await llama.extractText(imageData);
        // const levelInfo = extractLevelInfo(text);
        // console.log(`Extracted level info: ${JSON.stringify(levelInfo)}`);
      }
    }
  }
} finally {
  await obs.disconnect();
  llama.kill();
  process.exit();
}
