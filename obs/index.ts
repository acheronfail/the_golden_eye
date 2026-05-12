import { OBSWebSocket } from 'obs-websocket-js/msgpack';

import { Llama } from './llama';
import { extractLevelInfo } from './parse';
import { readEnv } from './envfile';

await readEnv();

console.time('llama');
const llama = new Llama();
await llama.initialised;
console.timeEnd('llama');

// TODO: upload to YT, something like https://github.com/jakzo/NeonWhiteMods/blob/main/scripts/upload-to-youtube.ts
const obs = new OBSWebSocket();
try {
  console.time('obs connect');
  await obs.connect('ws://localhost:4455', process.env.OBS_PASSWORD);
  console.timeEnd('obs connect');

  for (;;) {
    console.time('obs frame');
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });
    console.timeEnd('obs frame');

    if (await checkStartLevelScreen(imageData)) {
      console.time('extractText');
      const text = await llama.extractText(imageData);
      console.timeEnd('extractText');

      console.log('Extracted text:', text);

      // TODO: tell OBS start marker or something
    }

    if (await checkEndLevelScreen(imageData)) {
      console.time('extractText');
      const text = await llama.extractText(imageData);
      console.timeEnd('extractText');

      console.log('Extracted text:', text);
      console.log(extractLevelInfo(text));

      // TODO: tell OBS to save replay or something

      break;
    }
  }
} finally {
  await obs.disconnect();
  llama.kill();
}

process.exit();

async function checkStartLevelScreen(imageDataUrl: string): Promise<boolean> {
  return false;
}

async function checkEndLevelScreen(imageDataUrl: string): Promise<boolean> {
  return false;
}
