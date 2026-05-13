import cv from '@u4/opencv4nodejs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { matchThreshold, scale } from './common.ts';

const __dirname = dirname(fileURLToPath(import.meta.url));

let screen = null;
let image = null;

process.on('message', async (data) => {
  try {
    await handler(data);
  } catch (err) {
    console.error('Error handling message:', err);
  }
});

async function handler(data) {
  if (data.type === 'init') {
    const { filename } = data;
    screen = data.screen;
    image = cv.imread(join(__dirname, 'match-images', `${filename}.png`)).rescale(scale).cvtColor(cv.COLOR_BGR2GRAY);
    process.send({ type: 'init-complete' });
  }

  if (data.type === 'match' && image) {
    const { buffer, rows, cols, matType } = data;
    const sourceImage = new cv.Mat(buffer, rows, cols, matType);

    const result = sourceImage.matchTemplate(image, cv.TM_CCOEFF_NORMED);
    const { maxVal } = result.minMaxLoc();
    if (maxVal >= matchThreshold) {
      process.send({ type: 'match', maxVal, screen });
    } else {
      process.send({ type: 'match', maxVal, screen: null });
    }
  }
}
