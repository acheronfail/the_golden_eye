export const imageWidth = 400;
export const imageHeight = 300;
export const matchThreshold = 0.8;

export const allLangs = ['en', 'jp', 'emu-en', 'emu-jp'] as const;
export type Lang = typeof allLangs[number];
