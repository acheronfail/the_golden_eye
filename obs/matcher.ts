import cp from "node:child_process";
import { fileURLToPath } from "url";
import cv from "./opencv.ts";
import { imageScale, type Lang } from "./common.ts";
import type { MatcherProcessMessage } from "./matcher-process.ts";

// NOTE: order matters, since "EndLevelFailed" is a subset of "EndLevelComplete" when using the
// "mission-status" template.
const Screens = [
  "StartLevel",
  "EndLevelComplete",
  "EndLevelFailed",
  "EndLevelStats",
  "LevelSelect",
] as const;
export type Screen = (typeof Screens)[number];

// NOTE: double up for redundancy, in case the crosshair occludes part of the screen
const matchers: [Screen, string][] = [
  // no double up required since this template covers multiple areas of the screen
  ["LevelSelect", "level"],
  // no double up here since the cursor always starts to the right, so it will match
  // before the user has the chance to move it to obscure part of the screen
  ["EndLevelComplete", "complete"],
  ["EndLevelFailed", "kia"],
  ["EndLevelFailed", "abort"],
  ["EndLevelFailed", "failed"],
  ["EndLevelStats", "stats"],
  ["EndLevelStats", "stats-time"],
  ["StartLevel", "start-objectives"],
  ["StartLevel", "start"],
];

// NOTE: opencv4nodejs breaks when used in workers, so we create a process pool instead.
class Worker {
  process: cp.ChildProcess;

  constructor() {
    this.process = cp.fork(
      fileURLToPath(new URL("./matcher-process.ts", import.meta.url)),
      [],
      {
        serialization: "advanced",
      },
    );
  }

  async send(message: MatcherProcessMessage) {
    this.process.send!(message);
  }

  async init(filename: string, screen: Screen) {
    this.send({ type: "init", filename, screen });

    await new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject("worker process timed out"),
        10_000,
      );
      this.process.once("message", (message: any) => {
        if (message.type === "init-complete") {
          clearTimeout(timer);
          resolve(null);
        }
      });
    });
  }

  async match(
    buffer: Buffer,
    rows: number,
    cols: number,
    matType: number,
  ): Promise<{ maxVal: number; screen: Screen | null }> {
    this.send({ type: "match", buffer, rows, cols, matType });

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject("worker process timed out"), 1_000);
      this.process.once("message", (message: MatcherProcessMessage) => {
        if (message.type === "match-complete") {
          clearTimeout(timer);
          resolve(message);
        }
      });
    });
  }
}

export interface MatchResult {
  maxVal: number;
  screen: Screen;
  matcher: string;
}

export class MatcherProcessPool {
  private readonly workers: Worker[];
  private constructor(workers: Worker[]) {
    this.workers = workers;
  }

  public static async init(lang: Lang) {
    const workers = await Promise.all(
      matchers.map(async ([screen, filename]) => {
        const worker = new Worker();
        await worker.init(`${lang}-${filename}`, screen);
        worker.process.on("error", (err) =>
          console.error(`[worker:${screen}] error:`, err),
        );
        worker.process.on("exit", (code, signal) =>
          console.error(
            `[worker:${screen}] exited with code ${code} and signal ${signal}`,
          ),
        );

        return worker;
      }),
    );

    return new MatcherProcessPool(workers);
  }

  async matchScreen(jpegDataUri: string): Promise<MatchResult | null> {
    const jpegData = Buffer.from(jpegDataUri.split(",")[1], "base64");
    const sourceImage = cv
      .imdecode(jpegData)
      .rescale(imageScale)
      .cvtColor(cv.COLOR_BGR2GRAY);
    const { rows, cols, type } = sourceImage;
    const sourceData = sourceImage.getData();

    const results = await Promise.all<MatchResult | null>(
      this.workers.map(async (worker, i) => {
        const { maxVal, screen } = await worker.match(
          sourceData,
          rows,
          cols,
          type,
        );
        if (screen) {
          return { maxVal, screen, matcher: matchers[i][1] };
        }

        return null;
      }),
    );

    return results.reduce(
      (best, current) => {
        if (!current) {
          return best;
        }

        if (!best || current.maxVal > best.maxVal) {
          return current;
        }

        return best;
      },
      null as MatchResult | null,
    );
  }

  kill() {
    this.workers.forEach((worker) => worker.process.kill());
  }
}
