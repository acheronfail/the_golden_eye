import fs from 'node:fs/promises'
import { OBSWebSocket } from 'obs-websocket-js/msgpack';
import { readEnv } from '../obs/envfile';
import { matchScreen } from '../obs/matcher';
import { Llama } from '../obs/llama';

await readEnv();

const llama = new Llama();
await llama.initialised;
const obs = new OBSWebSocket();

const quit = async () => {
    await obs.disconnect();
    process.exit(0);
};

let screenshotPrefix = 'screenshot';
const screenshot = async () => {
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'png',
    });

    const fileName = `${screenshotPrefix}-${Date.now()}.png`;
    await fs.writeFile(fileName, Buffer.from(imageData.split(',')[1], 'base64'));
    process.stderr.write(`Saved ${fileName}\n`);
};

const match = async () => {
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });

    const screen = await matchScreen(imageData);
    console.log(`Matched screen: ${screen}`);
};

const watch = async () => {
    let done = false;
    process.stdin.once('data', () => done = true);

    while (!done) {
        await match();
        await new Promise((resolve) => setTimeout(resolve, 100));
    }
}

const text = async () => {
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });

    const text = await llama.extractText(imageData);
    console.log(`Extracted text: ${text}`);
};

const printHelp = () => process.stderr.write(`Available commands:
- (empty), save:    Take a screenshot and save it to the current directory
- name <prefix>:    Update the screenshot filename prefix (currently: "${screenshotPrefix}")
- match:            Take a screenshot and check if it's the start or end level screen
- text:             Read text from image
- h, help:          Show this help message
- q, exit, quit:    Quit the application
`);

try {
  console.time('obs connect');
  await obs.connect('ws://localhost:4455', process.env.OBS_PASSWORD);
  console.timeEnd('obs connect');

  const prompt = `enter command (h for help): `;

  process.stderr.write(prompt);
  for await (const chunk of process.stdin) {
        const command: string = chunk.toString().trim();


        const handler: (() => void) | undefined = {
            "": screenshot,
            "save": screenshot,
            "m": match,
            "match": match,
            "watch": watch,
            "text": text,
            "h": printHelp,
            "help": printHelp,
            "q": quit,
            "exit": quit,
            "quit": quit,
        }[command];

        if (handler) {
            const start = performance.now();
            await handler();
            const end = performance.now();
            process.stderr.write(`Command "${command}" executed in ${(end - start).toFixed(2)}ms\n`);
        } else {
            if (command.startsWith("name ")) {
                screenshotPrefix = command.slice(5).trim();
                process.stderr.write(`Screenshot prefix updated to "${screenshotPrefix}"\n`);
                process.stderr.write(prompt);
                continue;
            }

            printHelp();
        }

        process.stderr.write(prompt);
    }

} finally {
  await obs.disconnect();
  llama.kill();
}
