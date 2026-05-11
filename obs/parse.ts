const Levels = [
  ['Dam', 'Facility', 'Runway'],
  ['Surface 1', 'Bunker 1'],
  ['Silo'],
  ['Frigate'],
  ['Surface 2', 'Bunker 2'],
  ['Statue', 'Archives', 'Streets', 'Depot', 'Train'],
  ['Jungle', 'Control', 'Caverns', 'Cradle'],
  ['Aztec'],
  ['Egypt'],
] as const;
type Level = (typeof Levels)[number][number];

const Difficulties = ['secret agent', '00 agent', '007', 'agent'] as const;
type Difficulty = (typeof Difficulties)[number];

const parseTime = (time: string) => {
  const [minutes, seconds] = time.split(':').map(Number);
  return minutes * 60 + seconds;
};

export interface LevelInfo {
  difficulty: Difficulty;
  level: Level;
  time: number;
  bestTime: number;
}

export function extractLevelInfo(text: string): LevelInfo {
  const lowered = text.toLocaleLowerCase();

  const difficulty = Difficulties.find((d) => lowered.slice(0, 50).includes(d));
  const mission = lowered.match(/mission (\d+):/)?.[1];
  const partNumerals = lowered.match(/part ([ivxl]+):/)?.[1];
  const part = ['i', 'ii', 'iii', 'iv', 'v'].indexOf(partNumerals!);
  const timeString = lowered.match(/time: (\d+:\d+)/)?.[1];
  const bestTimeString = lowered.match(/best time: (\d+:\d+)/)?.[1];

  if (!difficulty || !mission || part === -1 || !timeString || !bestTimeString) {
    throw new Error('Failed to extract level info');
  }

  return {
    difficulty,
    level: Levels[parseInt(mission) - 1][part],
    time: parseTime(timeString),
    bestTime: parseTime(bestTimeString),
  };
}
