import * as cp from "node:child_process";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { promisify } from "node:util";
import chalk from "chalk";
import ora from "ora";
import { Bench } from "tinybench";
import { runners, type Runner } from "./runners.ts";
import { getScreenshots, type ScreenshotInfo } from "./screenshots.ts";

const [filter] = process.argv.slice(2);
const filterRe = filter?.trim() ? new RegExp(filter) : null;
const samplesPerScenario = parsePositiveIntEnv("GE_CV_BENCH_SAMPLES", 25);
const targetWarmups = parsePositiveIntEnv("GE_CV_BENCH_WARMUPS", 5);
const captureMode = process.env.GE_CV_BENCH_CAPTURE?.trim() || "obs";
const screenshots = (await getScreenshots()).sort((a, b) => a.filePath.localeCompare(b.filePath));

interface Scenario {
  key: string;
  target: ScreenshotInfo;
  warm: ScreenshotInfo | null;
  screenshots: ScreenshotInfo[];
}

interface BenchmarkPayload {
  cache_warm: any[];
  warmups: any[];
  samples: any[];
  opencv: string;
  image: { path: string; width: number; height: number };
  bench_image: { width: number; height: number };
  bench_capture: {
    mode: string;
    work_height: number;
    capture_region: null | {
      crop_x: number;
      crop_y: number;
      crop_w: number;
      crop_h: number;
      out_aspect: number;
    };
  };
}

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

async function execCommand(command: string, env: NodeJS.ProcessEnv) {
  try {
    return await promisify(cp.exec)(command, { env, maxBuffer: 20 * 1024 * 1024 });
  } catch (error) {
    console.error(chalk.red(`[cmd]: ${error instanceof Error ? error.message : String(error)}`));
    throw error;
  }
}

const scenarioKey = (screenshot: ScreenshotInfo): string => `${screenshot.tag}/${screenshot.lang}/${screenshot.screen}`;
const fileName = (screenshot: ScreenshotInfo): string => path.basename(screenshot.filePath, ".png");
const overlayScreens = new Set(["abort", "complete", "failed", "kia", "start", "stats"]);
const preferredWarmScreens = ["start", "stats", "complete", "failed", "abort", "kia"];

function chooseWarmFrame(target: ScreenshotInfo): ScreenshotInfo | null {
  if (overlayScreens.has(target.screen)) {
    return target;
  }
  return (
    preferredWarmScreens
      .map((screen) =>
        screenshots.find(
          (candidate) => candidate.tag === target.tag && candidate.lang === target.lang && candidate.screen === screen,
        ),
      )
      .find(Boolean) ?? null
  );
}

function buildScenarios(): Scenario[] {
  const groups = Map.groupBy(screenshots, scenarioKey);
  return [...groups.entries()]
    .map(([key, group]) => {
      const target = group[0];
      return { key, target, warm: chooseWarmFrame(target), screenshots: group };
    })
    .filter((scenario) => {
      if (!filterRe) {
        return true;
      }
      return (
        filterRe.exec(scenario.key) !== null ||
        filterRe.exec(scenario.target.filePath) !== null ||
        (scenario.warm ? filterRe.exec(scenario.warm.filePath) !== null : false)
      );
    })
    .sort((a, b) => a.key.localeCompare(b.key));
}

function percentile(sorted: readonly number[], p: number): number | undefined {
  if (sorted.length === 0) {
    return undefined;
  }

  const index = (sorted.length - 1) * p;
  const lower = Math.floor(index);
  const upper = Math.ceil(index);
  if (lower === upper) {
    return sorted[lower];
  }

  const weight = index - lower;
  return sorted[lower] * (1 - weight) + sorted[upper] * weight;
}

function ms(value: number | undefined): string {
  return value === undefined ? "-" : `${value.toFixed(2)} ms`;
}

function latencySummary(task: Bench["tasks"][number]) {
  if (!("latency" in task.result)) {
    return null;
  }

  const { samples, ...latency } = task.result.latency;
  const sortedSamples = samples ? [...samples].sort((a, b) => a - b) : [];
  return {
    ...latency,
    p90: percentile(sortedSamples, 0.9),
    p95: percentile(sortedSamples, 0.95),
    samples,
  };
}

function sortBySlowest<T extends { key: string; stats: { mean?: number } | null }>(items: readonly T[]): T[] {
  return [...items].sort(
    (a, b) => (b.stats?.mean ?? -Infinity) - (a.stats?.mean ?? -Infinity) || a.key.localeCompare(b.key),
  );
}

function tableRows(bench: Bench): Record<string, Record<string, string | number | undefined>> {
  const rows = bench.tasks.map((task) => ({
    key: task.name,
    stats: latencySummary(task),
  }));

  return Object.fromEntries(
    sortBySlowest(rows).map(({ key, stats }) => {
      return [
        key,
        {
          Mean: ms(stats?.mean),
          Median: ms(stats?.p50),
          P75: ms(stats?.p75),
          P90: ms(stats?.p90),
          P95: ms(stats?.p95),
          P99: ms(stats?.p99),
          Min: ms(stats?.min),
          Max: ms(stats?.max),
          RME: stats ? `${stats.rme.toFixed(2)}%` : "-",
          Samples: stats?.samplesCount,
        },
      ] as const;
    }),
  );
}

function progressText(completed: number, total: number, runner: Runner): string {
  return `Running ${completed}/${total} benchmark scenarios for ${chalk.cyan.bold(runner.name)}`;
}

function printConfigHints(scenarioCount: number) {
  console.log(
    chalk.blue(
      `Benchmarking ${scenarioCount} scenarios with ${samplesPerScenario} samples, ${targetWarmups} warmups, ${captureMode} capture mode.`,
    ),
  );
  console.log(
    chalk.gray(
      "Tune with GE_CV_BENCH_SAMPLES=N, GE_CV_BENCH_WARMUPS=N, GE_CV_BENCH_CAPTURE=obs|fixture, and an optional regex filter argument.",
    ),
  );
}

async function runMatcherBenchmark(runner: Runner, scenario: Scenario): Promise<BenchmarkPayload> {
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    GE_CV_BENCH: String(samplesPerScenario),
    GE_CV_BENCH_CAPTURE: captureMode,
    GE_CV_BENCH_JSON: "1",
    GE_CV_BENCH_WARMUPS: String(targetWarmups),
  };
  if (scenario.warm) {
    env.GE_CV_BENCH_WARM = scenario.warm.filePath;
  }

  const { stdout } = await execCommand(runner.command(scenario.target.filePath, scenario.target.lang), env);
  return JSON.parse(stdout);
}

const scenarios = buildScenarios();
printConfigHints(scenarios.length);

const results: Record<string, { scenarios: any[] }> = {};
for (const runner of runners) {
  const payloads: BenchmarkPayload[] = [];
  const bench = new Bench({ iterations: samplesPerScenario, retainSamples: true, time: 0, warmup: false });
  let completedScenarios = 0;
  const spinner = ora(chalk.blue(progressText(completedScenarios, scenarios.length, runner))).start();

  try {
    for (const scenario of scenarios) {
      const payload = await runMatcherBenchmark(runner, scenario);
      const runtimes = payload.samples.map((sample) => sample.runtime_ms);
      let index = 0;

      payloads.push(payload);
      bench.add(scenario.key, () => ({ overriddenDuration: runtimes[index++ % runtimes.length] }), { async: false });
      completedScenarios += 1;
      spinner.text = chalk.blue(progressText(completedScenarios, scenarios.length, runner));
    }
    spinner.succeed(
      chalk.blue(
        `Finished ${completedScenarios}/${scenarios.length} benchmark scenarios for ${chalk.cyan.bold(runner.name)}`,
      ),
    );
  } catch (error) {
    spinner.fail(
      chalk.red(`Failed after ${completedScenarios}/${scenarios.length} benchmark scenarios for ${runner.name}`),
    );
    throw error;
  }

  await bench.run();
  console.log(chalk.blue(`Benchmark results for ${chalk.cyan.bold(runner.name)}...`));
  console.table(tableRows(bench));

  const scenarioResults = scenarios.map((scenario, index) => {
    const payload = payloads[index];
    const task = bench.tasks[index];
    const stats = latencySummary(task);
    return {
      key: scenario.key,
      runner: runner.name,
      frame_count: scenario.screenshots.length,
      target: { name: fileName(scenario.target), path: scenario.target.filePath },
      cache_warm: scenario.warm
        ? {
            kind: scenario.warm.filePath === scenario.target.filePath ? "target" : "overlay",
            name: fileName(scenario.warm),
            path: scenario.warm.filePath,
            screen: scenario.warm.screen,
            results: payload.cache_warm,
          }
        : null,
      warmups: { count: payload.warmups.length, results: payload.warmups },
      samples: { count: payload.samples.length, results: payload.samples },
      stats,
      opencv: payload.opencv,
      image: payload.image,
      bench_image: payload.bench_image,
      bench_capture: payload.bench_capture,
    };
  });

  results[runner.name] = {
    scenarios: sortBySlowest(scenarioResults),
  };
}

await fs.writeFile(
  "bench_results.json",
  JSON.stringify(
    {
      config: {
        grouping: ["screenshot folder", "lang", "screen"],
        samples_per_scenario: samplesPerScenario,
        target_warmups_per_scenario: targetWarmups,
        bench_capture_mode: captureMode,
        available_parallelism: os.availableParallelism?.() ?? os.cpus().length,
        cache_warm_strategy:
          "Use the target frame when it is an overlay; otherwise use a same-folder/lang overlay frame to prime aspect and scale caches.",
        scenario_count: scenarios.length,
      },
      results,
    },
    null,
    2,
  ),
  "utf-8",
);
console.log(chalk.blue(`Benchmark results written to ${chalk.cyan.bold("bench_results.json")}`));
