import cv from '@u4/opencv4nodejs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const scale = 0.25;
const matchThreshold = 0.8;

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

    process.send({ type: 'init-complete' });
  }

  if (data.type === 'match' && image) {
    const { imageDataUrl } = data;
    const base64Data = imageDataUrl.replace(/^data:image\/\w+;base64,/, '');
    const buffer = Buffer.from(base64Data, 'base64');
    const sourceImage = cv.imdecode(buffer).rescale(scale);

    const result = sourceImage.matchTemplate(image, cv.TM_CCOEFF_NORMED);
    const { maxVal } = result.minMaxLoc();
    if (maxVal >= matchThreshold) {
      process.send({ type: 'match', screen });
    } else {
      process.send({ type: 'match', screen: null });
    }
  }
}
