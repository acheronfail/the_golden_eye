import cp from 'node:child_process';
import { fileURLToPath } from 'node:url';

import type { LevelInfo } from './parse.ts';

export class UploaderProcess {
  process: cp.ChildProcess;

  constructor() {
    this.process = cp.fork(fileURLToPath(new URL('./upload-process.ts', import.meta.url)), [], {
      serialization: 'advanced',
    });
  }

  async init() {
    this.process.send({ type: 'init' });

    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject('uploader process timed out'), 10_000);
      this.process.once('message', (message: any) => {
        if (message.type === 'init-complete') {
          clearTimeout(timer);
          resolve(null);
        }
      });
    });
  }

  uploadRecording(filePath: string, levelInfo: LevelInfo) {
    this.process.send({ type: 'upload', filePath, levelInfo });
  }
}
