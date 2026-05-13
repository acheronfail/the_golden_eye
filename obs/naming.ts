import {
  DifficiultyNumberMap,
  LevelNumberMap,
  type Difficulty,
  type Level,
  type LevelInfo,
} from "./parse.ts";

const separator = " - ";

export function createVideoFileName(levelInfo: LevelInfo): string {
  const formattedTime = `${Math.floor(levelInfo.time / 60)
    .toString()
    .padStart(2, "0")}:${(levelInfo.time % 60).toString().padStart(2, "0")}`;

  return [
    levelInfo.levelNumber.toString().padStart(2, "0"),
    levelInfo.level,
    levelInfo.difficulty,
    formattedTime,
    new Date().toISOString(),
  ].join(separator);
}

export interface VideoNameParts {
  levelNumber: number;
  level: string;
  difficulty: string;
  difficultyNumber: number;
  time: string;
  date: Date;
}

export function parseVideoName(videoName: string): VideoNameParts | null {
  const parts = videoName.split(separator);
  if (parts.length !== 5) {
    return null;
  }

  const [levelNumberStr, level, difficulty, time, dateStr] = parts;
  const levelNumber = parseInt(levelNumberStr, 10);
  if (isNaN(levelNumber)) {
    return null;
  }

  const difficultyNumber = DifficiultyNumberMap.get(difficulty as Difficulty);
  if (difficultyNumber === undefined) {
    return null;
  }

  const date = new Date(dateStr);
  if (isNaN(date.getTime())) {
    return null;
  }

  return {
    levelNumber,
    level,
    difficulty,
    difficultyNumber,
    time,
    date,
  };
}

export interface YoutubeVideoInfo {
  title: string;
  description: string;
}

export function createYoutubeTitle(
  nameParts: VideoNameParts,
  extraTag?: string,
): YoutubeVideoInfo {
  const { level, difficulty, time, date } = nameParts;
  const title = [level, difficulty, time, ...(extraTag ? [extraTag] : [])].join(separator);
  const description = `Date achieved: ${date.toLocaleString([], {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  })}`;

  return { title, description };
}

export interface ParsedYoutubeTitle {
  level: string;
  levelNumber: number;
  difficulty: string;
  difficultyNumber: number;
  time: string;
}
export function parseYoutubeTitle(title: string): ParsedYoutubeTitle | null {
  const [level, difficulty, time] = title.split(separator);
  if (!level || !difficulty || !time) {
    return null;
  }

  const levelNumber = LevelNumberMap.get(level as Level);
  if (levelNumber === undefined) {
    return null;
  }

  const difficultyNumber = DifficiultyNumberMap.get(difficulty as Difficulty);
  if (difficultyNumber === undefined) {
    return null;
  }

  return { level, levelNumber, difficulty, difficultyNumber, time };
}
