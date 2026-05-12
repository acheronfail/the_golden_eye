import fs from 'node:fs/promises'
import { OBSWebSocket } from 'obs-websocket-js/msgpack';
import { readEnv } from '../obs/envfile';
import { matchScreen } from '../obs/matcher';

await readEnv();

function printHelp() {
    process.stderr.write(`Available commands:
- (empty), save:    Take a screenshot and save it to the current directory
- name:             Update the screenshot filename prefix (default: "screenshot")
- match:            Take a screenshot and check if it's the start or end level screen
- h, help:          Show this help message
- q, exit, quit:    Quit the application
`);
}

const obs = new OBSWebSocket();

const quit = async () => {
    await obs.disconnect();
    process.exit(0);
};

let screenshotPrefix = 'screenshot';
const screenshot = async () => {
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });

    const fileName = `${screenshotPrefix}-${Date.now()}.jpg`;
    await fs.writeFile(fileName, Buffer.from(imageData.split(',')[1], 'base64'));
    process.stderr.write(`Saved ${fileName}\n`);
};

const match = async () => {
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: process.env.SOURCE_NAME,
      imageFormat: 'jpg',
    });

    const screen = await matchScreen(imageData);
    if (screen) {
        process.stderr.write(`Current screen: ${screen}\n`);
    } else {
        process.stderr.write(`Could not match current screen\n`);
    }
};

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
            "match": match,
            "h": printHelp,
            "help": printHelp,
            "q": quit,
            "exit": quit,
            "quit": quit,
        }[command];

        if (handler) {
            await handler();
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

//   for (;;) {


    // console.time('obs frame');
    const { imageData } = await obs.call('GetSourceScreenshot', {
      sourceName: 'Capture Card Device',
      imageFormat: 'jpg',
    });
    // console.timeEnd('obs frame');
//   }
} finally {
  await obs.disconnect();
}
