const Levels = [
  ["Dam", "Facility", "Runway"],
  ["Surface 1", "Bunker 1"],
  ["Silo"],
  ["Frigate"],
  ["Surface 2", "Bunker 2"],
  ["Statue", "Archives", "Streets", "Depot", "Train"],
  ["Jungle", "Control", "Caverns", "Cradle"],
  ["Aztec"],
  ["Egyptian"],
] as const;
export type Level = (typeof Levels)[number][number];

export const LevelNumberMap = new Map(Levels.flat().map((level, i) => [level, i + 1]));
export const NumberLevelMap = new Map(Array.from(LevelNumberMap.entries()).map(([level, num]) => [num, level]));

export const getLevel = (missionNumber: number, partNumber: number): Level | null => {
  const level = Levels[missionNumber - 1]?.[partNumber - 1];
  return level || null;
};
