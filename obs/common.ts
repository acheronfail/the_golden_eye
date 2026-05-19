export const imageWidth = 800;
export const imageHeight = 600;
export const imageScale = 0.5;
export const matchThreshold = 0.8;

export const allLangs = ['en', 'jp', 'emu-en', 'emu-jp'] as const;
export type Lang = typeof allLangs[number];
