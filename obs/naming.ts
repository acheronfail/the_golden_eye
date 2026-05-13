import type { LevelInfo } from './parse.ts';

const separator = ' | ';

export function createVideoFileName(levelInfo: LevelInfo): string {
  const formattedTime = `${Math.floor(levelInfo.time / 60)
    .toString()
    .padStart(2, '0')}:${(levelInfo.time % 60).toString().padStart(2, '0')}`;

  return [
    levelInfo.levelNumber.toString().padStart(2, '0'),
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

  const date = new Date(dateStr);
  if (isNaN(date.getTime())) {
    return null;
  }

  return {
    levelNumber,
    level,
    difficulty,
    time,
    date,
  };
}
