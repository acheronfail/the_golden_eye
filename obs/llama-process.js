import { Llama } from "./llama";
import { extractLevelInfo } from "./parse";

const llama = new Llama();
await llama.initialised;

process.send({ type: 'ready' });

// TODO: do this on a background worker/thread to not block
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
  console.log(`Extracted level info: ${JSON.stringify(levelInfo)}`);

  // TODO: control OBS and save reply buffer or something to that effect
  // TODO: upload to YT, something like https://github.com/jakzo/NeonWhiteMods/blob/main/scripts/upload-to-youtube.ts
}
