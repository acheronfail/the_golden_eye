import * as cp from "node:child_process";
import * as path from "node:path";
import * as os from "node:os";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import chalk from "chalk";
import { describe, it, expect, beforeAll } from "vitest";
import { getScreenshots } from "./screenshots.ts";
import { getLevel } from "./levels.ts";
import { NumberDifficultyMap } from "./difficulty.ts";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rustRoot = path.join(__dirname, "..", "obs2", "rust");

// spawns `command: string` and returns a promise that resolves with the stdout and stderr
const execCommand = (command: string): Promise<{ stdout: string; stderr: string }> => {
  const cwd = process.cwd();
  const home = os.homedir();
  console.log(chalk.green(`[cmd]: ${chalk.yellow(command.replace(cwd, ".").replace(home, "~"))}`));
  return promisify(cp.exec)(command).catch((error) => {
    console.error(chalk.red(`[cmd]: ${error.message}`));
    throw error;
  });
};

interface Runner {
  name: string;
  build?: (debug: boolean) => string;
  command: (debug: boolean, screenshotPath: string) => string;
}

const runners: Runner[] = [
  {
    name: "cv templates",
    build: (debug) => `cd "${rustRoot}" && cargo build --bin test_match ${debug ? "" : "--release"}`,
    command: (debug, sp) => `"${rustRoot}/target/${debug ? "debug" : "release"}/test_match" "${sp}"`,
  },
];

const debug = "DEBUG" in process.env;
const screenshots = await getScreenshots();

describe("frames", () => {
  for (const runner of runners) {
    describe(runner.name, () => {
      beforeAll(async () => {
        if (runner.build) {
          await execCommand(runner.build(debug));
        }
      }, Infinity);

      for (const screenshot of screenshots) {
        if (screenshot.screen === "levels") {
          // TODO: implement "screen" to match these
          continue;
        }

        it(screenshot.name, async () => {
          const { stdout } = await execCommand(runner.command(debug, screenshot.filePath));
          const result = JSON.parse(stdout);
          const resultLevel = getLevel(result.mission, result.part);

          expect(result.lang).toBe(screenshot.lang);
          expect(resultLevel).toBe(screenshot.level);

          if (screenshot.screen === "stats") {
            const [timesStr] = screenshot.extra;
            const times = timesStr.split("_").map((digits) => {
              const mm = digits.slice(0, 2);
              const ss = digits.slice(2, 4);
              return parseInt(mm, 10) * 60 + parseInt(ss, 10);
            });

            expect(result.times).toEqual(times);
          }

          if (screenshot.screen !== "select" && screenshot.screen !== "levels") {
            const resultDifficulty = NumberDifficultyMap.get(result.difficulty);
            expect(resultDifficulty).toBe(screenshot.difficulty);
          }
        });
      }
    });
  }
});
