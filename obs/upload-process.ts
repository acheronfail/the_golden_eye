process.on('message', (data) => {
  if (data.type === 'init') {
    process.send({ type: 'init-complete' });
  }

  if (data.type === 'upload') {
    // TODO: https://github.com/jakzo/NeonWhiteMods/blob/main/scripts/upload-to-youtube.ts
    const { filePath, levelInfo } = data.payload;
  }
});
