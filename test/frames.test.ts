import * as fs from "node:fs/promises";
import * as cp from "node:child_process";
import { promisify } from "node:util";
import chalk from "chalk";
import stripAnsi from "strip-ansi";
import { getScreenshots } from "./screenshots.ts";
import { getLevel } from "./levels.ts";
import { abbrDifficulty, NumberDifficultyMap, type Difficulty } from "./difficulty.ts";
import { runners } from "./runners.ts";

const [filter] = process.argv.slice(2);
const filterRe = filter?.trim() ? new RegExp(filter) : null;

const execCommand = async (command: string) => {
  try {
    return await promisify(cp.exec)(command);
  } catch (error) {
    console.error(chalk.red(`[cmd]: ${error instanceof Error ? error.message : String(error)}`));
    throw error;
  }
};

const screenshots = await getScreenshots();

interface CheckResult {
  value: any;
  expected: any;
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
  screen?: CheckResult;
  level?: CheckResult;
  difficulty?: CheckResult;
  times?: CheckResult;
  runTime: number;
  runTimePass?: boolean;
}

const lengthName = Math.max(...screenshots.map((s) => s.tag.length + ": ".length + s.name.length), "Test".length);
const lengthLang = 6; // " Lang "
const lengthScreen = 9; // " 007opts "
const lengthLevel = 11; // " Surface 2 "
const lengthDifficulty = 12; // " Difficulty "
const lengthTimes = 13; // " SSS,SSS,SSS "
const lengthRuntime = 10; // " 1234.56 ms "
const lengthWidth =
  lengthName +
  lengthLang +
  lengthScreen +
  lengthLevel +
  lengthDifficulty +
  lengthTimes +
  lengthRuntime +
  14 /* padding */ +
  6; /* separators */

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

interface Screenshot {
  name: string;
  results: TestResult[];
}
const results: Record<
  string,
  { results: Screenshot[]; totalTests: number; totalChecks: number; passedChecks: number; skippedTests: number }
> = {};
for (const runner of runners) {
  results[runner.name] = {
    totalChecks: 0,
    totalTests: 0,
    passedChecks: 0,
    skippedTests: 0,
    results: [],
  };
  console.log(chalk.blue(`Running tests for ${chalk.cyan.bold(runner.name)}...`));

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
            h("Screen", lengthScreen),
            h("Level", lengthLevel),
            h("Difficulty", lengthDifficulty),
            h("Times", lengthTimes),
            h("Runtime", lengthRuntime),
          ].join(" │ ") +
          ` │`,
      ),
    );
  }

  for (const screenshot of screenshots) {
    const screenshotResult: Screenshot = { name: screenshot.tag + ": " + screenshot.name, results: [] };

    if (filterRe ? filterRe.exec(screenshot.filePath) === null : false) {
      results[runner.name].skippedTests++;
      continue;
    }

    results[runner.name].totalTests++;

    const { stdout } = await execCommand(runner.command(screenshot.filePath, screenshot.lang));

    const result = JSON.parse(stdout);
    const testResult: TestResult = { runTime: result.runtime_ms };

    testResult.lang = { value: result.lang, pass: result.lang === screenshot.lang, expected: screenshot.lang };
    results[runner.name].totalChecks += 1;

    testResult.screen = {
      value: result.screen,
      pass: result.screen === screenshot.screen,
      expected: screenshot.screen,
    };
    results[runner.name].totalChecks += 1;

    if (["stats", "start", "complete", "failed", "abort", "kia"].includes(screenshot.screen)) {
      const resultLevel = getLevel(result.mission, result.part);
      testResult.level = { value: resultLevel, pass: resultLevel === screenshot.level, expected: screenshot.level };
      results[runner.name].totalChecks += 1;

      let resultDifficulty: Difficulty | undefined;
      resultDifficulty = NumberDifficultyMap.get(result.difficulty);
      testResult.difficulty = {
        value: abbrDifficulty(resultDifficulty),
        pass: resultDifficulty === screenshot.difficulty,
        expected: abbrDifficulty(screenshot.difficulty),
      };
      results[runner.name].totalChecks += 1;
    }

    // `result.times` is the classified `{ time, target_time, best_time }` object,
    // whereas `result.raw_times` is the unclassified top-to-bottom list the
    // matcher read off the overlay. The tests validate digit reading, so they
    // compare `raw_times` against the times the screenshot filename encodes --
    // classification is verified separately by the Rust unit tests.
    if (screenshot.screen === "stats") {
      const [timesStr] = screenshot.extra;
      const times = timesStr.split("_").map((digits) => {
        const mm = digits.slice(0, 2);
        const ss = digits.slice(2, 4);
        return parseInt(mm, 10) * 60 + parseInt(ss, 10);
      });

      testResult.times = {
        value: result.raw_times,
        pass: JSON.stringify(result.raw_times) === JSON.stringify(times),
        expected: times,
      };
      results[runner.name].totalChecks += 1;
    } else {
      testResult.times = {
        value: result.raw_times,
        pass: Array.isArray(result.raw_times) && result.raw_times.length === 0,
        expected: [],
      };
      testResult.runTimePass = screenshot.tag === "emu" || screenshot.tag === "rt4kce" ? true : result.runtime_ms < 16;
      results[runner.name].totalChecks += 2;
    }

    {
      const name = padText(chalk.white(screenshot.tag + ": " + screenshot.name), lengthName, "left");
      const lang = padText(formatCheckResult(testResult.lang), lengthLang);
      const screen = padText(formatCheckResult(testResult.screen), lengthScreen);
      const level = padText(formatCheckResult(testResult.level), lengthLevel);
      const difficulty = padText(formatCheckResult(testResult.difficulty), lengthDifficulty);
      const times = padText(formatCheckResult(testResult.times), lengthTimes);
      const runTimeText = testResult.runTime.toFixed(2) + " ms";
      const execTime = padText(
        (testResult.runTimePass === false ? chalk.red : chalk.white)(runTimeText),
        lengthRuntime,
      );
      console.log(chalk.grey(`│ ${name} │ ${lang} │ ${screen} │ ${level} │ ${difficulty} │ ${times} │ ${execTime} │`));
      results[runner.name].passedChecks += [
        testResult.lang,
        testResult.screen,
        testResult.level,
        testResult.difficulty,
        testResult.times,
      ].filter((r) => r?.pass).length;
      if (testResult.runTimePass !== undefined && testResult.runTimePass) {
        results[runner.name].passedChecks += 1;
      }

      // Only add failing tests to the results.
      const didFail = [
        testResult.difficulty?.pass === false,
        testResult.lang?.pass === false,
        testResult.screen?.pass === false,
        testResult.level?.pass === false,
        testResult.runTimePass === false,
        testResult.times?.pass === false,
      ].some((result) => result);

      if (didFail) {
        if (!results[runner.name].results.includes(screenshotResult)) {
          results[runner.name].results.push(screenshotResult);
        }

        screenshotResult.results.push(testResult);
      }
    }
  }

  console.log(chalk.grey(`└${"─".repeat(lengthWidth)}┘`));
  // log the % of all passed tests
  {
    const passed = chalk.green.bold(results[runner.name].passedChecks);
    const total = chalk.bold(results[runner.name].totalChecks);
    const pct = (results[runner.name].passedChecks / results[runner.name].totalChecks) * 100;
    const pctStr = (pct === 100 ? chalk.green : chalk.red)(`${pct.toFixed(2)}%`);
    console.log(chalk.blue(`Passed ${passed} out of ${total} checks: ${pctStr}`));
    console.log(
      chalk.blue(`Total tests run: ${results[runner.name].totalTests} (skipped: ${results[runner.name].skippedTests})`),
    );
  }

  console.log();
}

await fs.writeFile("test_results.json", JSON.stringify({ results }, null, 2), "utf-8");
console.log(chalk.blue(`Test results written to ${chalk.cyan.bold("test_results.json")}`));
