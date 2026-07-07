import * as fs from "node:fs/promises";
import * as cp from "node:child_process";
import * as path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
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
const testRoot = path.dirname(fileURLToPath(import.meta.url));

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
  detectedLang?: CheckResult;
  screen?: CheckResult;
  level?: CheckResult;
  difficulty?: CheckResult;
  times?: CheckResult;
  runTime: number;
  runTimeUnderTarget?: boolean;
}

interface FailedCheck {
  runner: string;
  test: string;
  check: string;
  value: any;
  expected: any;
}

const RUNTIME_TARGET_MS = 16;

const languageMismatchCases = [
  {
    name: "language mismatch: English start with Japanese templates",
    filePath: "screenshots-emu/en - start - 01 - Agent.png",
    configuredLang: "jp",
    detectedLang: "en",
  },
  {
    name: "language mismatch: Japanese start with English templates",
    filePath: "screenshots-emu/jp - start - 01 - Agent.png",
    configuredLang: "en",
    detectedLang: "jp",
  },
  {
    name: "language mismatch: English blackbar start with Japanese templates",
    filePath: "screenshots-av2hdmi/en - start - 3 - 00 Agent - blackbars.png",
    configuredLang: "jp",
    detectedLang: "en",
  },
];

const lengthName = Math.max(
  ...screenshots.map((s) => s.tag.length + ": ".length + s.name.length),
  ...languageMismatchCases.map((c) => c.name.length),
  "Test".length,
);
const lengthLang = 6; // " Lang "
const lengthDetected = 10; // " Detected "
const lengthScreen = 9; // " 007opts "
const lengthLevel = 11; // " Surface 2 "
const lengthDifficulty = 12; // " Difficulty "
const lengthTimes = 13; // " SSS,SSS,SSS "
const lengthRuntime = 10; // " 1234.56 ms "
const lengthWidth =
  lengthName +
  lengthLang +
  lengthDetected +
  lengthScreen +
  lengthLevel +
  lengthDifficulty +
  lengthTimes +
  lengthRuntime +
  16 /* padding */ +
  7; /* separators */

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
  {
    results: Screenshot[];
    totalTests: number;
    totalChecks: number;
    passedChecks: number;
    skippedTests: number;
    runtimeUnderTarget: number;
    runtimeTargetTotal: number;
  }
> = {};
const failedChecks: FailedCheck[] = [];
for (const runner of runners) {
  results[runner.name] = {
    totalChecks: 0,
    totalTests: 0,
    passedChecks: 0,
    skippedTests: 0,
    runtimeUnderTarget: 0,
    runtimeTargetTotal: 0,
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
            h("Detected", lengthDetected),
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

    if (screenshot.screen === "detail") {
      testResult.screen = {
        value: result.screen,
        pass: result.screen !== "stats",
        expected: "anything except 'detail'",
      };
      results[runner.name].totalChecks += 1;
    } else {
      testResult.screen = {
        value: result.screen,
        pass: result.screen === screenshot.screen,
        expected: screenshot.screen,
      };
      results[runner.name].totalChecks += 1;
    }

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
      if (screenshot.tag !== "emu" && screenshot.tag !== "rt4kce") {
        testResult.runTimeUnderTarget = result.runtime_ms < RUNTIME_TARGET_MS;
        results[runner.name].runtimeTargetTotal += 1;
        if (testResult.runTimeUnderTarget) {
          results[runner.name].runtimeUnderTarget += 1;
        }
      }
      results[runner.name].totalChecks += 1;
    }

    {
      const name = padText(chalk.white(screenshot.tag + ": " + screenshot.name), lengthName, "left");
      const lang = padText(formatCheckResult(testResult.lang), lengthLang);
      const detectedLang = padText(formatCheckResult(testResult.detectedLang), lengthDetected);
      const screen = padText(formatCheckResult(testResult.screen), lengthScreen);
      const level = padText(formatCheckResult(testResult.level), lengthLevel);
      const difficulty = padText(formatCheckResult(testResult.difficulty), lengthDifficulty);
      const times = padText(formatCheckResult(testResult.times), lengthTimes);
      const runTimeText = testResult.runTime.toFixed(2) + " ms";
      const execTime = padText(
        (testResult.runTimeUnderTarget === false ? chalk.yellow : chalk.white)(runTimeText),
        lengthRuntime,
      );
      console.log(
        chalk.grey(
          `│ ${name} │ ${lang} │ ${detectedLang} │ ${screen} │ ${level} │ ${difficulty} │ ${times} │ ${execTime} │`,
        ),
      );
      const correctnessChecks = [
        testResult.lang,
        testResult.detectedLang,
        testResult.screen,
        testResult.level,
        testResult.difficulty,
        testResult.times,
      ];
      results[runner.name].passedChecks += correctnessChecks.filter((r) => r?.pass).length;

      // Only add failing tests to the results.
      const failedCorrectnessChecks = [
        ["lang", testResult.lang],
        ["detectedLang", testResult.detectedLang],
        ["screen", testResult.screen],
        ["level", testResult.level],
        ["difficulty", testResult.difficulty],
        ["times", testResult.times],
      ].filter((entry): entry is [string, CheckResult] => {
        const [, result] = entry;
        return typeof result === "object" && result?.pass === false;
      });

      const didFail = failedCorrectnessChecks.length > 0;

      if (didFail) {
        if (!results[runner.name].results.includes(screenshotResult)) {
          results[runner.name].results.push(screenshotResult);
        }

        screenshotResult.results.push(testResult);
        for (const [check, result] of failedCorrectnessChecks) {
          failedChecks.push({
            runner: runner.name,
            test: screenshotResult.name,
            check,
            value: result.value,
            expected: result.expected,
          });
        }
      }
    }
  }

  for (const mismatch of languageMismatchCases) {
    const filePath = path.join(testRoot, mismatch.filePath);
    const screenshotResult: Screenshot = { name: mismatch.name, results: [] };

    if (filterRe ? filterRe.exec(filePath) === null : false) {
      results[runner.name].skippedTests++;
      continue;
    }

    results[runner.name].totalTests++;

    const { stdout } = await execCommand(runner.command(filePath, mismatch.configuredLang));
    const result = JSON.parse(stdout);
    const testResult: TestResult = { runTime: result.runtime_ms };

    testResult.lang = {
      value: result.lang,
      pass: result.lang === mismatch.configuredLang,
      expected: mismatch.configuredLang,
    };
    testResult.detectedLang = {
      value: result.detected_lang,
      pass: result.detected_lang === mismatch.detectedLang,
      expected: mismatch.detectedLang,
    };
    testResult.screen = {
      value: result.screen,
      pass: result.screen === "unknown",
      expected: "unknown",
    };
    testResult.times = {
      value: result.raw_times,
      pass: Array.isArray(result.raw_times) && result.raw_times.length === 0 && result.times === null,
      expected: [],
    };
    results[runner.name].totalChecks += 4;

    const name = padText(chalk.white(mismatch.name), lengthName, "left");
    const lang = padText(formatCheckResult(testResult.lang), lengthLang);
    const detectedLang = padText(formatCheckResult(testResult.detectedLang), lengthDetected);
    const screen = padText(formatCheckResult(testResult.screen), lengthScreen);
    const level = padText(formatCheckResult(testResult.level), lengthLevel);
    const difficulty = padText(formatCheckResult(testResult.difficulty), lengthDifficulty);
    const times = padText(formatCheckResult(testResult.times), lengthTimes);
    const execTime = padText(chalk.white(testResult.runTime.toFixed(2) + " ms"), lengthRuntime);
    console.log(
      chalk.grey(
        `│ ${name} │ ${lang} │ ${detectedLang} │ ${screen} │ ${level} │ ${difficulty} │ ${times} │ ${execTime} │`,
      ),
    );

    const correctnessChecks = [testResult.lang, testResult.detectedLang, testResult.screen, testResult.times];
    results[runner.name].passedChecks += correctnessChecks.filter((r) => r?.pass).length;

    const failedCorrectnessChecks = [
      ["lang", testResult.lang],
      ["detectedLang", testResult.detectedLang],
      ["screen", testResult.screen],
      ["times", testResult.times],
    ].filter((entry): entry is [string, CheckResult] => {
      const [, result] = entry;
      return typeof result === "object" && result?.pass === false;
    });

    if (failedCorrectnessChecks.length > 0) {
      results[runner.name].results.push(screenshotResult);
      screenshotResult.results.push(testResult);
      for (const [check, result] of failedCorrectnessChecks) {
        failedChecks.push({
          runner: runner.name,
          test: screenshotResult.name,
          check,
          value: result.value,
          expected: result.expected,
        });
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
    const runtimeTotal = results[runner.name].runtimeTargetTotal;
    const runtimePassed = results[runner.name].runtimeUnderTarget;
    const runtimePct = runtimeTotal === 0 ? 100 : (runtimePassed / runtimeTotal) * 100;
    console.log(
      chalk.blue(
        `Runtime under ${RUNTIME_TARGET_MS} ms: ${chalk.bold(runtimePassed)} out of ${chalk.bold(runtimeTotal)} (${runtimePct.toFixed(2)}%)`,
      ),
    );
  }

  console.log();
}

await fs.writeFile("test_results.json", JSON.stringify({ results }, null, 2), "utf-8");
console.log(chalk.blue(`Test results written to ${chalk.cyan.bold("test_results.json")}`));

if (failedChecks.length > 0) {
  console.log();
  console.log(chalk.red.bold(`Failed checks (${failedChecks.length}):`));
  for (const failure of failedChecks) {
    console.log(
      chalk.red(
        `- ${failure.runner} / ${failure.test} / ${failure.check}: got ${JSON.stringify(failure.value)}, expected ${JSON.stringify(failure.expected)}`,
      ),
    );
  }
  process.exitCode = 1;
}
