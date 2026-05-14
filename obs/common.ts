export const scale = 0.15;
export const matchThreshold = 0.8;

export const allLangs = ['en', 'jp', 'emu-en', 'emu-jp'] as const;
export type Lang = typeof allLangs[number];
