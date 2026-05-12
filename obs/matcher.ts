import cp from 'node:child_process';
import { fileURLToPath } from 'url';
import cv from '@u4/opencv4nodejs';

const Screens = ['StartLevel', 'EndLevelComplete', 'EndLevel', 'EndLevelStats'] as const;
export type Screen = (typeof Screens)[number];

// NOTE: opencv4nodejs breaks when used in workers, so we create a process pool instead.

class Worker {
  process: cp.ChildProcess;

  constructor() {
    this.process = cp.fork(fileURLToPath(new URL('./matcher-process.js', import.meta.url)), [], {
      serialization: 'advanced',
    });
  }

  async init(filename: string, screen: Screen, cropRegion: [number, number, number, number]) {
    this.process.send({ type: 'init', filename, screen, cropRegion });

    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject('worker process timed out'), 1000);
      this.process.once('message', (message: any) => {
        if (message.type === 'init-complete') {
          clearTimeout(timer);
          resolve(null);
        }
      });
    });
  }

  async match(buffer: Buffer, rows: number, cols: number, matType: number): Promise<Screen | null> {
    this.process.send({ type: 'match', buffer, rows, cols, matType });

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject('worker process timed out'), 1000);
      this.process.once('message', (message: any) => {
        if (message.type === 'match') {
          clearTimeout(timer);
          resolve(message.screen);
        }
      });
    });
  }
}

// FIXME: use 2 separate templates per screen, so if the cursor covers one we still get a match
const workers: Worker[] = await Promise.all(
  (
    [
      ['mission-status-completed', 'EndLevelComplete', [0, 0, 1, 0.5]],
      ['mission-status', 'EndLevel', [0, 0, 1, 0.5]],
      ['statistics', 'EndLevelStats', [0, 0, 1, 0.5]],
      ['primary-objectives', 'StartLevel', [0, 0, 1, 0.5]],
    ] as [string, Screen, [number, number, number, number]][]
  ).map(async ([filename, screen, cropRegion]) => {
    const worker = new Worker();
    await worker.init(filename, screen, cropRegion);
    worker.process.on('error', (err) => console.error(`[worker:${screen}] error:`, err));
    worker.process.on('exit', (code, signal) =>
      console.error(`[worker:${screen}] exited with code ${code} and signal ${signal}`),
    );

    return worker;
  }),
);

export async function matchScreen(jpegDataUri: string): Promise<Screen | null> {
  const jpegData = Buffer.from(jpegDataUri.split(',')[1], 'base64');
  const sourceImage = cv.imdecode(jpegData).rescale(0.25).cvtColor(cv.COLOR_BGR2GRAY);
  const { rows, cols, type } = sourceImage;
  const sourceData = sourceImage.getData();

  const results = await Promise.all<Screen | null>(workers.map((worker) => worker.match(sourceData, rows, cols, type)));

  return results.find((result) => result !== null) ?? null;
}
