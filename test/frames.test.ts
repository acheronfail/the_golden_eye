import * as cp from "node:child_process";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import chalk from "chalk";
import ora from "ora";
import { abbrDifficulty, NumberDifficultyMap, type Difficulty } from "./difficulty.ts";
import { getLevel } from "./levels.ts";
import { runners, type Runner } from "./runners.ts";
import { getScreenshots, type ScreenshotInfo } from "./screenshots.ts";

const [filter] = process.argv.slice(2);
const filterRe = filter?.trim() ? new RegExp(filter) : null;
const screenshots = await getScreenshots();
const testRoot = path.dirname(fileURLToPath(import.meta.url));
const testJobs = parsePositiveIntEnv(
  "GE_CV_TEST_JOBS",
  Math.max(1, Math.floor((os.availableParallelism?.() ?? os.cpus().length) / 2)),
);

interface CheckResult {
  value: any;
  expected: any;
  pass: boolean;
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
  checks: Record<string, CheckResult | undefined>;
}

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
type TestCase =
  | { kind: "screenshot"; screenshot: ScreenshotInfo }
  | { kind: "language-mismatch"; mismatch: LanguageMismatchCase };

function parsePositiveIntEnv(name: string, defaultValue: number): number {
  const raw = process.env[name]?.trim();
  if (!raw) {
    return defaultValue;
  }

  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < 1) {
    throw new Error(`${name} must be a positive integer, got ${JSON.stringify(raw)}`);
  }
  return parsed;
}

async function execCommand(command: string) {
  try {
    return await promisify(cp.exec)(command);
  } catch (error) {
    console.error(chalk.red(`[cmd]: ${error instanceof Error ? error.message : String(error)}`));
    throw error;
  }
}

async function mapConcurrent<T, R>(
  items: readonly T[],
  limit: number,
  mapper: (item: T, index: number) => Promise<R>,
): Promise<R[]> {
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
}

const check = (value: any, expected: any, pass: boolean): CheckResult => ({
  value,
  expected,
  pass,
});

function passed(checks: Record<string, CheckResult | undefined>): number {
  return Object.values(checks).filter((result) => result?.pass).length;
}

function failedEntries(checks: Record<string, CheckResult | undefined>): [string, CheckResult][] {
  return Object.entries(checks).filter(
    (entry): entry is [string, CheckResult] => entry[1]?.pass === false,
  );
}

function progressText(completed: number, total: number, runner: Runner, jobs: number): string {
  return `Running ${completed}/${total} tests for ${chalk.cyan.bold(runner.name)} with up to ${jobs} jobs`;
}

function printConfigHints(testCount: number) {
  console.log(chalk.blue(`Running ${testCount} CV tests with up to ${testJobs} jobs.`));
  console.log(chalk.gray("Tune with GE_CV_TEST_JOBS=N and an optional regex filter argument."));
}

async function evaluateScreenshotTest(
  runner: Runner,
  screenshot: ScreenshotInfo,
): Promise<EvaluatedTest> {
  const { stdout } = await execCommand(runner.command(screenshot.filePath, screenshot.lang));
  const result = JSON.parse(stdout);
  const checks: Record<string, CheckResult | undefined> = {};

  checks.lang = check(result.lang, screenshot.lang, result.lang === screenshot.lang);
  checks.screen =
    screenshot.screen === "detail"
      ? check(result.screen, "anything except 'detail'", result.screen !== "stats")
      : check(result.screen, screenshot.screen, result.screen === screenshot.screen);

  if (["stats", "start", "complete", "failed", "abort", "kia"].includes(screenshot.screen)) {
    const resultLevel = getLevel(result.mission, result.part);
    const resultDifficulty: Difficulty | undefined = NumberDifficultyMap.get(result.difficulty);
    checks.level = check(resultLevel, screenshot.level, resultLevel === screenshot.level);
    checks.difficulty = check(
      abbrDifficulty(resultDifficulty),
      abbrDifficulty(screenshot.difficulty),
      resultDifficulty === screenshot.difficulty,
    );
  }

  if (screenshot.screen === "stats") {
    const [timesStr] = screenshot.extra;
    const times = timesStr.split("_").map((digits) => {
      const mm = digits.slice(0, 2);
      const ss = digits.slice(2, 4);
      return parseInt(mm, 10) * 60 + parseInt(ss, 10);
    });
    checks.times = check(
      result.raw_times,
      times,
      JSON.stringify(result.raw_times) === JSON.stringify(times),
    );
  } else {
    checks.times = check(
      result.raw_times,
      [],
      Array.isArray(result.raw_times) && result.raw_times.length === 0,
    );
  }

  return { name: `${screenshot.tag}: ${screenshot.name}`, checks };
}

async function evaluateLanguageMismatchTest(
  runner: Runner,
  mismatch: LanguageMismatchCase,
): Promise<EvaluatedTest> {
  const filePath = path.join(testRoot, mismatch.filePath);
  const { stdout } = await execCommand(runner.command(filePath, mismatch.configuredLang));
  const result = JSON.parse(stdout);

  return {
    name: mismatch.name,
    checks: {
      lang: check(result.lang, mismatch.configuredLang, result.lang === mismatch.configuredLang),
      detectedLang: check(
        result.detected_lang,
        mismatch.detectedLang,
        result.detected_lang === mismatch.detectedLang,
      ),
      screen: check(result.screen, "unknown", result.screen === "unknown"),
      times: check(
        result.raw_times,
        [],
        Array.isArray(result.raw_times) && result.raw_times.length === 0 && result.times === null,
      ),
    },
  };
}

const results: Record<
  string,
  {
    results: { name: string; results: Record<string, CheckResult | undefined>[] }[];
    totalTests: number;
    totalChecks: number;
    passedChecks: number;
    skippedTests: number;
  }
> = {};
const failedChecks: FailedCheck[] = [];

const screenshotCases = screenshots.filter((screenshot) =>
  filterRe ? filterRe.exec(screenshot.filePath) !== null : true,
);
const mismatchCases = languageMismatchCases.filter((mismatch) => {
  const filePath = path.join(testRoot, mismatch.filePath);
  return filterRe ? filterRe.exec(filePath) !== null : true;
});
printConfigHints(screenshotCases.length + mismatchCases.length);

for (const runner of runners) {
  const activeJobs = Math.min(testJobs, Math.max(1, screenshotCases.length + mismatchCases.length));
  const testCases: TestCase[] = [
    ...screenshotCases.map((screenshot) => ({ kind: "screenshot" as const, screenshot })),
    ...mismatchCases.map((mismatch) => ({ kind: "language-mismatch" as const, mismatch })),
  ];

  let completedTests = 0;
  const spinner = ora(
    chalk.blue(progressText(completedTests, testCases.length, runner, activeJobs)),
  ).start();
  let evaluated: EvaluatedTest[];
  try {
    evaluated = await mapConcurrent(testCases, testJobs, async (testCase) => {
      const result =
        testCase.kind === "screenshot"
          ? await evaluateScreenshotTest(runner, testCase.screenshot)
          : await evaluateLanguageMismatchTest(runner, testCase.mismatch);
      completedTests += 1;
      spinner.text = chalk.blue(progressText(completedTests, testCases.length, runner, activeJobs));
      return result;
    });
    spinner.succeed(
      chalk.blue(
        `Finished ${completedTests}/${testCases.length} tests for ${chalk.cyan.bold(runner.name)}`,
      ),
    );
  } catch (error) {
    spinner.fail(
      chalk.red(`Failed after ${completedTests}/${testCases.length} tests for ${runner.name}`),
    );
    throw error;
  }

  const runnerResults = {
    totalChecks: 0,
    totalTests: evaluated.length,
    passedChecks: 0,
    skippedTests: screenshots.length + languageMismatchCases.length - evaluated.length,
    results: [] as { name: string; results: Record<string, CheckResult | undefined>[] }[],
  };

  for (const test of evaluated) {
    const failures = failedEntries(test.checks);
    runnerResults.totalChecks += Object.values(test.checks).filter(Boolean).length;
    runnerResults.passedChecks += passed(test.checks);

    if (failures.length > 0) {
      runnerResults.results.push({ name: test.name, results: [test.checks] });
      for (const [checkName, result] of failures) {
        failedChecks.push({
          runner: runner.name,
          test: test.name,
          check: checkName,
          value: result.value,
          expected: result.expected,
        });
      }
    }
  }

  results[runner.name] = runnerResults;
  const pct =
    runnerResults.totalChecks === 0
      ? 100
      : (runnerResults.passedChecks / runnerResults.totalChecks) * 100;
  const pctStr = (pct === 100 ? chalk.green : chalk.red)(`${pct.toFixed(2)}%`);
  console.log(
    chalk.blue(
      `Passed ${chalk.green.bold(runnerResults.passedChecks)} out of ${chalk.bold(runnerResults.totalChecks)} checks: ${pctStr}`,
    ),
  );
  console.log(
    chalk.blue(
      `Total tests run: ${runnerResults.totalTests} (skipped: ${runnerResults.skippedTests})`,
    ),
  );
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
