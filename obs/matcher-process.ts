import cv, { Mat } from '@u4/opencv4nodejs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { Screen } from './matcher';

const __dirname = dirname(fileURLToPath(import.meta.url));
const scale = 0.25;
const matchThreshold = 0.8;

let screen: Screen | null = null;
let image: Mat | null = null;

let chunkBuffer = '';
process.stdin.setEncoding('utf-8');
process.stdin.on('data', async (chunk) => {
  try {
    chunkBuffer += chunk;
    let boundary = chunkBuffer.indexOf('\n');
    while (boundary !== -1) {
      const data = JSON.parse(chunkBuffer.slice(0, boundary));
      await handler(data);
      chunkBuffer = chunkBuffer.slice(boundary + 1);
      boundary = chunkBuffer.indexOf('\n');
    }
  } catch (err) {
    console.error('Error handling message:', err);
  }
});

async function handler(data: any) {
  if (data.type === 'init') {
    const { filename, cropRegion } = data;
    screen = data.screen;

    const img = cv.imread(join(__dirname, 'match-images', `${filename}.png`)).rescale(scale);
    const [cx, cy, cw, ch] = cropRegion;
    const rect = new cv.Rect(
      Math.floor(cx * img.cols),
      Math.floor(cy * img.rows),
      Math.ceil(cw * img.cols),
      Math.ceil(ch * img.rows),
    );
    image = img.getRegion(rect);

    process.stderr.write(JSON.stringify({ type: 'init-complete' }) + '\n');
  }

  if (data.type === 'match' && image) {
    const { imageDataUrl } = data;
    const base64Data = imageDataUrl.replace(/^data:image\/\w+;base64,/, '');
    const buffer = Buffer.from(base64Data, 'base64');
    const sourceImage = cv.imdecode(buffer).rescale(scale);

    const result = sourceImage.matchTemplate(image, cv.TM_CCOEFF_NORMED);
    const { maxVal } = result.minMaxLoc();
    if (maxVal >= matchThreshold) {
      process.stderr.write(JSON.stringify({ type: 'match', screen }) + '\n');
    } else {
      process.stderr.write(JSON.stringify({ type: 'match', screen: null }) + '\n');
    }
  }
}
