import type { LlamaParseResult } from "./llama-process.ts";

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

const Difficulties = ["Secret Agent", "00 Agent", "007", "Agent"] as const;
export type Difficulty = (typeof Difficulties)[number];
export const DifficiultyNumberMap = new Map<Difficulty, number>([
  ["Agent", 0],
  ["Secret Agent", 1],
  ["00 Agent", 2],
  ["007", 3],
]);

const parseTime = (time: string) => {
  const [minutes, seconds] = time.split(":").map(Number);
  return minutes * 60 + seconds;
};

export interface LevelInfo {
  difficulty: Difficulty;
  level: Level;
  levelNumber: number;
  time: number;
  bestTime?: number;
}

export function extractLevelInfo(llamaResult: LlamaParseResult): {
  levelInfo: LevelInfo;
  llamaResult: LlamaParseResult;
} {
  const lowered = llamaResult.text.toLocaleLowerCase();

  const parsedDifficulty = Difficulties.find((d) => lowered.slice(0, 50).includes(d.toLowerCase()));
  const mission = lowered.match(/mission[\s:]*(\d+):/)?.[1];
  const partNumerals = lowered.match(/part[\s:]*([ivxl]+):/)?.[1];
  const part = ["i", "ii", "iii", "iv", "v"].indexOf(partNumerals!);
  const timeString = lowered.match(/time: (\d+:\d+)/)?.[1];
  const bestTimeString = lowered.match(/best time: (\d+:\d+)/)?.[1];
  const level = mission && Levels[parseInt(mission) - 1]?.[part];

  // TODO: make a note when parse result is different to this match - show that in the info box onscreen?

  if (!parsedDifficulty || !level || !timeString) {
    console.error({ difficulty: parsedDifficulty, mission, part, timeString, bestTimeString }, llamaResult);
    throw new Error("Failed to extract level info");
  }

  return {
    llamaResult,
    levelInfo: {
      difficulty: parsedDifficulty,
      level,
      levelNumber: LevelNumberMap.get(level)!,
      time: parseTime(timeString),
      bestTime: bestTimeString ? parseTime(bestTimeString) : undefined,
    },
  };
}
