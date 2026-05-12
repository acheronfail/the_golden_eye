import { Llama } from "./llama";
import { extractLevelInfo } from "./parse";

const llama = new Llama();
await llama.initialised;

process.send({ type: 'ready' });

process.on('message', async (data) => {
  try {
    const { type, imageData } = data;
    if (type === 'extract-level-info') {
      await handleLevelComplete(imageData);
    }
  } catch (err) {
    console.error('Error handling message:', err);
  }
});

export async function handleLevelComplete(jpegDataUri) {
  const text = await llama.extractText(jpegDataUri);
  const levelInfo = extractLevelInfo(text);
  process.send({ type: 'level-info', levelInfo });
}
