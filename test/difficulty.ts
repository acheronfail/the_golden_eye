export const Difficulties = ["Secret Agent", "00 Agent", "007", "Agent"] as const;
export type Difficulty = (typeof Difficulties)[number];

export const DifficultyNumberMap = new Map<Difficulty, number>([
  ["Agent", 0],
  ["Secret Agent", 1],
  ["00 Agent", 2],
  ["007", 3],
]);
export const NumberDifficultyMap = new Map<number, Difficulty>([
  [0, "Agent"],
  [1, "Secret Agent"],
  [2, "00 Agent"],
  [3, "007"],
]);

export const JpDifficultyMap = new Map<string, Difficulty>([
  ["スパイ", "Agent"],
  ["特命スパイ", "Secret Agent"],
  ["<00Agent>", "00 Agent"],
  ["007", "007"],
]);
