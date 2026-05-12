import cp from 'node:child_process';

const Screens = ['StartLevel', 'EndLevelComplete', 'EndLevel', 'EndLevelStats'] as const;
export type Screen = (typeof Screens)[number];

// NOTE: opencv4nodejs breaks when used in workers, so we create a process pool instead.

class Worker {
  process: cp.ChildProcessWithoutNullStreams;

  constructor() {
    this.process = cp.spawn('npx', ['tsx', 'obs/matcher-process.ts']);
    this.process.stdout.pipe(process.stdout);
  }

  async init(filename: string, screen: Screen, cropRegion: [number, number, number, number]) {
    this.process.stdin.write(JSON.stringify({ type: 'init', filename, screen, cropRegion }) + '\n');

    await new Promise((resolve) => {
      this.process.stderr.once('data', (data) => {
        const message = JSON.parse(data.toString());
        if (message.type === 'init-complete') {
          resolve(null);
        }
      });
    });
  }

  async match(imageDataUrl: string): Promise<Screen | null> {
    this.process.stdin.write(JSON.stringify({ type: 'match', imageDataUrl }) + '\n');

    return new Promise((resolve) => {
      this.process.stderr.once('data', (data) => {
        const message = JSON.parse(data.toString());
        if (message.type === 'match') {
          resolve(message.screen);
        }
      });
    });
  }
}

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
    worker.process.on('exit', (code, signal) => console.error(`[worker:${screen}] exited with code ${code} and signal ${signal}`));

    return worker;
  }),
);

export async function matchScreen(imageDataUrl: string): Promise<Screen | null> {
  const base64Data = imageDataUrl.replace(/^data:image\/\w+;base64,/, '');
  const buffer = Buffer.from(base64Data, 'base64');
  const sharedBuffer = new SharedArrayBuffer(buffer.length);
  const sharedArray = new Uint8Array(sharedBuffer);
  sharedArray.set(buffer);

  const results = await Promise.all<Screen | null>(
    workers.map(
      (worker) => worker.match(imageDataUrl),
    ),
  );

  return results.find((result) => result !== null) ?? null;
}
