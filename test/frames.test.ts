import * as cp from "node:child_process";
import * as path from "node:path";
import * as os from "node:os";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import chalk from "chalk";
import stripAnsi from "strip-ansi";
import { getScreenshots } from "./screenshots.ts";
import { getLevel } from "./levels.ts";
import { abbrDifficulty, NumberDifficultyMap, type Difficulty } from "./difficulty.ts";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rustRoot = path.join(__dirname, "..", "obs2", "rust");

const execCommand = async (command: string) => {
  try {
    return await promisify(cp.exec)(command);
  } catch (error) {
    console.error(chalk.red(`[cmd]: ${error instanceof Error ? error.message : String(error)}`));
    throw error;
  }
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

interface CheckResult {
  value: any;
  pass: boolean;
}

const formatCheckResult = (result: CheckResult | undefined): string => {
  if (!result) {
    return chalk.grey("-");
  }
  return result.pass ? chalk.green(result.value) : chalk.red(result.value);
};

interface TestResult {
  lang?: CheckResult;
  level?: CheckResult;
  difficulty?: CheckResult;
  times?: CheckResult;
  runTime: number;
}

const lengthName = Math.max(...screenshots.map((s) => s.name.length), "Test".length);
const lengthLang = 6; // " Lang "
const lengthLevel = 11; // " Surface 2 "
const lengthDifficulty = 12; // " Difficulty "
const lengthTimes = 13; // " SSS,SSS,SSS "
const lengthRuntime = 10; // " 1234.56 ms "
const lengthWidth =
  lengthName +
  lengthLang +
  lengthLevel +
  lengthDifficulty +
  lengthTimes +
  lengthRuntime +
  12 /* padding */ +
  5; /* separators */

const padText = (text: string, width: number, align: "left" | "center" | "right" = "center"): string => {
  const padding = Math.max(0, width - stripAnsi(text).length);

  if (align === "left") {
    return text + " ".repeat(padding);
  } else if (align === "right") {
    return " ".repeat(padding) + text;
  } else {
    // center
    const padStart = Math.floor(padding / 2);
    const padEnd = padding - padStart;
    return " ".repeat(padStart) + text + " ".repeat(padEnd);
  }
};

for (const runner of runners) {
  console.log(chalk.blue(`Running tests for ${chalk.cyan.bold(runner.name)}...`));
  if (runner.build) {
    await execCommand(runner.build(debug));
  }

  // ┌─────────────────────────┐
  // │      Sample table       │
  // ├───────┬─────┬───────────┤
  // │ Name  │ Age │ Eye color │
  // ├───────┼─────┼───────────┤
  // │ John  │  23 │   green   │
  // │ Mary  │  16 │   brown   │
  // │ Rita  │  47 │   blue    │
  // │ Peter │   8 │   brown   │
  // └───────┴─────┴───────────┘
  {
    console.log(chalk.grey(`┌${"─".repeat(lengthWidth)}┐`));
    const h = (text: string, w: number = 0) => chalk.white.bold(padText(text, w));
    console.log(
      chalk.grey(
        "│ " +
          [
            h("Test", lengthName),
            h("Lang", lengthLang),
            h("Level", lengthLevel),
            h("Difficulty", lengthDifficulty),
            h("Times", lengthTimes),
            h("Runtime", lengthRuntime),
          ].join(" │ ") +
          ` │`,
      ),
    );
  }

  let totalTests = 0;
  let passedTests = 0;
  for (const screenshot of screenshots) {
    if (screenshot.screen === "levels") {
      // TODO: implement "screen" to match these
      continue;
    }

    process.env.GE_LANG = screenshot.lang;
    const { stdout } = await execCommand(runner.command(debug, screenshot.filePath)).finally(() => {
      delete process.env.GE_LANG;
    });

    const result = JSON.parse(stdout);
    const resultLevel = getLevel(result.mission, result.part);
    const testResult: TestResult = { runTime: result.runtime_ms };

    testResult.lang = { value: result.lang, pass: result.lang === screenshot.lang };
    testResult.level = { value: resultLevel, pass: resultLevel === screenshot.level };
    totalTests += 2;

    if (screenshot.screen === "stats") {
      const [timesStr] = screenshot.extra;
      const times = timesStr.split("_").map((digits) => {
        const mm = digits.slice(0, 2);
        const ss = digits.slice(2, 4);
        return parseInt(mm, 10) * 60 + parseInt(ss, 10);
      });

      testResult.times = { value: result.times, pass: JSON.stringify(result.times) === JSON.stringify(times) };
      totalTests += 1;
    }

    let resultDifficulty: Difficulty | undefined;
    // @ts-expect-error we filter out levels above with a TODO
    if (screenshot.screen !== "select" && screenshot.screen !== "levels") {
      resultDifficulty = NumberDifficultyMap.get(result.difficulty);
      testResult.difficulty = {
        value: abbrDifficulty(resultDifficulty),
        pass: resultDifficulty === screenshot.difficulty,
      };
      totalTests += 1;
    }

    {
      const name = padText(chalk.white(screenshot.name), lengthName, "left");
      const lang = padText(formatCheckResult(testResult.lang), lengthLang);
      const level = padText(formatCheckResult(testResult.level), lengthLevel);
      const difficulty = padText(formatCheckResult(testResult.difficulty), lengthDifficulty);
      const times = padText(formatCheckResult(testResult.times), lengthTimes);
      const execTime = padText(chalk.white(testResult.runTime.toFixed(2) + " ms"), lengthRuntime);
      console.log(chalk.grey(`│ ${name} │ ${lang} │ ${level} │ ${difficulty} │ ${times} │ ${execTime} │`));
      passedTests += [testResult.lang, testResult.level, testResult.difficulty, testResult.times].filter(
        (r) => r?.pass,
      ).length;
    }
  }

  console.log(chalk.grey(`└${"─".repeat(lengthWidth)}┘`));
  // log debug mode
  console.log(chalk.blue(`Mode: ${debug ? chalk.yellow("Debug") : chalk.green("Release")}`));
  // log the % of all passed tests
  {
    const passed = chalk.green.bold(passedTests);
    const total = chalk.bold(totalTests);
    const pct = (passedTests / totalTests) * 100;
    const pctStr = (pct === 100 ? chalk.green : chalk.red)(`${pct.toFixed(2)}%`);
    console.log(chalk.blue(`Passed ${passed} out of ${total} tests: ${pctStr}`));
  }

  console.log();
}
