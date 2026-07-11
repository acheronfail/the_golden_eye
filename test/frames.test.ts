import * as fs from "node:fs/promises";
import * as cp from "node:child_process";
import * as os from "node:os";
import * as path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import chalk from "chalk";
import stripAnsi from "strip-ansi";
import { getScreenshots, type ScreenshotInfo } from "./screenshots.ts";
import { getLevel } from "./levels.ts";
import { abbrDifficulty, NumberDifficultyMap, type Difficulty } from "./difficulty.ts";
import { runners, type Runner } from "./runners.ts";

const [filter] = process.argv.slice(2);
const filterRe = filter?.trim() ? new RegExp(filter) : null;
const defaultJobs = Math.max(1, (os.availableParallelism?.() ?? os.cpus().length) / 2);
const testJobs = (() => {
  const raw = process.env.GE_CV_TEST_JOBS?.trim();
  if (!raw) {
    return defaultJobs;
  }

  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < 1) {
    throw new Error(`GE_CV_TEST_JOBS must be a positive integer, got ${JSON.stringify(raw)}`);
  }

  return parsed;
})();

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

interface EvaluatedTest {
  name: string;
  testResult: TestResult;
  totalChecks: number;
  passedChecks: number;
  failedCorrectnessChecks: [string, CheckResult][];
  runtimeUnderTarget: number;
  runtimeTargetTotal: number;
}

const RUNTIME_TARGET_MS = 16;

const mapConcurrent = async <T, R>(
  items: readonly T[],
  limit: number,
  mapper: (item: T, index: number) => Promise<R>,
): Promise<R[]> => {
  const results = new Array<R>(items.length);
  let nextIndex = 0;
  const workerCount = Math.min(Math.max(1, limit), Math.max(1, items.length));

  await Promise.all(
    Array.from({ length: workerCount }, async () => {
      while (true) {
        const index = nextIndex++;
        if (index >= items.length) {
          return;
        }

        results[index] = await mapper(items[index], index);
      }
    }),
  );

  return results;
};

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

type LanguageMismatchCase = (typeof languageMismatchCases)[number];

const getFailedCorrectnessChecks = (checks: [string, CheckResult | undefined][]): [string, CheckResult][] =>
  checks.filter((entry): entry is [string, CheckResult] => {
    const [, result] = entry;
    return typeof result === "object" && result?.pass === false;
  });

const countPassedChecks = (checks: (CheckResult | undefined)[]): number => checks.filter((r) => r?.pass).length;

const evaluateScreenshotTest = async (runner: Runner, screenshot: ScreenshotInfo): Promise<EvaluatedTest> => {
  const { stdout } = await execCommand(runner.command(screenshot.filePath, screenshot.lang));

  const result = JSON.parse(stdout);
  const testResult: TestResult = { runTime: result.runtime_ms };
  let totalChecks = 0;
  let runtimeUnderTarget = 0;
  let runtimeTargetTotal = 0;

  testResult.lang = { value: result.lang, pass: result.lang === screenshot.lang, expected: screenshot.lang };
  totalChecks += 1;

  if (screenshot.screen === "detail") {
    testResult.screen = {
      value: result.screen,
      pass: result.screen !== "stats",
      expected: "anything except 'detail'",
    };
    totalChecks += 1;
  } else {
    testResult.screen = {
      value: result.screen,
      pass: result.screen === screenshot.screen,
      expected: screenshot.screen,
    };
    totalChecks += 1;
  }

  if (["stats", "start", "complete", "failed", "abort", "kia"].includes(screenshot.screen)) {
    const resultLevel = getLevel(result.mission, result.part);
    testResult.level = { value: resultLevel, pass: resultLevel === screenshot.level, expected: screenshot.level };
    totalChecks += 1;

    let resultDifficulty: Difficulty | undefined;
    resultDifficulty = NumberDifficultyMap.get(result.difficulty);
    testResult.difficulty = {
      value: abbrDifficulty(resultDifficulty),
      pass: resultDifficulty === screenshot.difficulty,
      expected: abbrDifficulty(screenshot.difficulty),
    };
    totalChecks += 1;
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
    totalChecks += 1;
  } else {
    testResult.times = {
      value: result.raw_times,
      pass: Array.isArray(result.raw_times) && result.raw_times.length === 0,
      expected: [],
    };
    if (screenshot.tag !== "emu" && screenshot.tag !== "rt4kce") {
      testResult.runTimeUnderTarget = result.runtime_ms < RUNTIME_TARGET_MS;
      runtimeTargetTotal += 1;
      if (testResult.runTimeUnderTarget) {
        runtimeUnderTarget += 1;
      }
    }
    totalChecks += 1;
  }

  const correctnessChecks = [
    testResult.lang,
    testResult.detectedLang,
    testResult.screen,
    testResult.level,
    testResult.difficulty,
    testResult.times,
  ];

  return {
    name: screenshot.tag + ": " + screenshot.name,
    testResult,
    totalChecks,
    passedChecks: countPassedChecks(correctnessChecks),
    failedCorrectnessChecks: getFailedCorrectnessChecks([
      ["lang", testResult.lang],
      ["detectedLang", testResult.detectedLang],
      ["screen", testResult.screen],
      ["level", testResult.level],
      ["difficulty", testResult.difficulty],
      ["times", testResult.times],
    ]),
    runtimeUnderTarget,
    runtimeTargetTotal,
  };
};

const evaluateLanguageMismatchTest = async (runner: Runner, mismatch: LanguageMismatchCase): Promise<EvaluatedTest> => {
  const filePath = path.join(testRoot, mismatch.filePath);
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

  const correctnessChecks = [testResult.lang, testResult.detectedLang, testResult.screen, testResult.times];

  return {
    name: mismatch.name,
    testResult,
    totalChecks: 4,
    passedChecks: countPassedChecks(correctnessChecks),
    failedCorrectnessChecks: getFailedCorrectnessChecks([
      ["lang", testResult.lang],
      ["detectedLang", testResult.detectedLang],
      ["screen", testResult.screen],
      ["times", testResult.times],
    ]),
    runtimeUnderTarget: 0,
    runtimeTargetTotal: 0,
  };
};

const printTestRow = (nameText: string, testResult: TestResult) => {
  const name = padText(chalk.white(nameText), lengthName, "left");
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
};

const recordEvaluatedTest = (runner: Runner, evaluated: EvaluatedTest) => {
  results[runner.name].totalTests++;
  results[runner.name].totalChecks += evaluated.totalChecks;
  results[runner.name].passedChecks += evaluated.passedChecks;
  results[runner.name].runtimeUnderTarget += evaluated.runtimeUnderTarget;
  results[runner.name].runtimeTargetTotal += evaluated.runtimeTargetTotal;

  if (evaluated.failedCorrectnessChecks.length === 0) {
    return;
  }

  const screenshotResult: Screenshot = { name: evaluated.name, results: [evaluated.testResult] };
  results[runner.name].results.push(screenshotResult);
  for (const [check, result] of evaluated.failedCorrectnessChecks) {
    failedChecks.push({
      runner: runner.name,
      test: screenshotResult.name,
      check,
      value: result.value,
      expected: result.expected,
    });
  }
};

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
  const screenshotCases = screenshots.filter((screenshot) =>
    filterRe ? filterRe.exec(screenshot.filePath) !== null : true,
  );
  const mismatchCases = languageMismatchCases.filter((mismatch) => {
    const filePath = path.join(testRoot, mismatch.filePath);
    return filterRe ? filterRe.exec(filePath) !== null : true;
  });
  results[runner.name].skippedTests += screenshots.length - screenshotCases.length;
  results[runner.name].skippedTests += languageMismatchCases.length - mismatchCases.length;

  const activeJobs = Math.min(testJobs, Math.max(1, screenshotCases.length + mismatchCases.length));
  console.log(chalk.blue(`Running tests for ${chalk.cyan.bold(runner.name)}...`));
  console.log(
    chalk.blue(`Running ${screenshotCases.length + mismatchCases.length} tests with up to ${activeJobs} jobs...`),
  );

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

  const screenshotEvaluations = await mapConcurrent(screenshotCases, testJobs, (screenshot) =>
    evaluateScreenshotTest(runner, screenshot),
  );
  for (const evaluated of screenshotEvaluations) {
    printTestRow(evaluated.name, evaluated.testResult);
    recordEvaluatedTest(runner, evaluated);
  }

  const mismatchEvaluations = await mapConcurrent(mismatchCases, testJobs, (mismatch) =>
    evaluateLanguageMismatchTest(runner, mismatch),
  );
  for (const evaluated of mismatchEvaluations) {
    printTestRow(evaluated.name, evaluated.testResult);
    recordEvaluatedTest(runner, evaluated);
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
