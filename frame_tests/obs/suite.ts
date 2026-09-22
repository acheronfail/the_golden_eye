import * as fs from "node:fs/promises";
import path from "node:path";

export interface Scenario<H> {
  name: string;
  run: (harness: H) => Promise<void>;
}
export interface Session {
  launch(): Promise<void>;
  close(): Promise<void>;
  ready?(): Promise<void>;
}
export interface Failure {
  phase: "launch" | "test" | "cleanup";
  message: string;
  stack?: string;
  details?: unknown;
  causes?: Omit<Failure, "phase">[];
}
export interface ScenarioResult {
  name: string;
  iteration: number;
  status: "passed" | "failed" | "skipped";
  elapsedMs: number;
  artifacts: string;
  errors: Failure[];
}

function describe(error: unknown): Omit<Failure, "phase"> {
  return error instanceof Error
    ? {
        message: error.message,
        stack: error.stack,
        details: "details" in error ? error.details : undefined,
        causes: error instanceof AggregateError ? error.errors.map(describe) : undefined,
      }
    : { message: String(error) };
}

export async function runSuite<H extends Session>(options: {
  artifacts: string;
  scenarios: Scenario<H>[];
  repeats: number;
  create: (directory: string) => H;
  interrupted: () => boolean;
  log?: (message: string) => void;
}) {
  const log = options.log ?? console.log;
  const tests: ScenarioResult[] = [];
  const report = { passed: 0, failed: 0, skipped: 0, tests };
  await fs.mkdir(options.artifacts, { recursive: true });
  const saveReport = async () => {
    for (const status of ["passed", "failed", "skipped"] as const)
      report[status] = tests.filter((test) => test.status === status).length;
    for (const [index, test] of tests.entries()) {
      await fs.writeFile(
        path.join(options.artifacts, `case-${index + 1}.json`),
        JSON.stringify(test, null, 2),
      );
    }
    await fs.writeFile(
      path.join(options.artifacts, "result.json"),
      JSON.stringify(report, null, 2),
    );
  };
  for (let iteration = 1; iteration <= options.repeats; iteration++) {
    let harness: H | undefined;
    let directory = "";
    let lastResult: ScenarioResult | undefined;
    const close = async (result: ScenarioResult) => {
      const current = harness;
      harness = undefined;
      try {
        await current?.close();
      } catch (error) {
        result.errors.push({ phase: "cleanup", ...describe(error) });
        result.status = "failed";
        log(`  cleanup: ${result.errors.at(-1)!.message}`);
      }
    };
    try {
      for (const [index, scenario] of options.scenarios.entries()) {
        const result: ScenarioResult = {
          name: scenario.name,
          iteration,
          status: "skipped",
          elapsedMs: 0,
          artifacts: "",
          errors: [],
        };
        tests.push(result);
        if (options.interrupted()) {
          if (harness && lastResult) await close(lastResult);
          await saveReport();
          continue;
        }
        lastResult = result;
        log(`RUN ${iteration}/${options.repeats}: ${scenario.name}`);
        const start = performance.now();
        let phase: Failure["phase"] = "launch";
        try {
          if (!harness) {
            directory = path.join(options.artifacts, `session-${iteration}-${index + 1}`);
            await fs.mkdir(directory, { recursive: true });
            harness = options.create(directory);
            await harness.launch();
          }
          phase = "test";
          await scenario.run(harness);
          await harness.ready?.();
          if (options.interrupted()) throw new Error("Test interrupted");
        } catch (error) {
          result.errors.push({ phase, ...describe(error) });
        } finally {
          result.artifacts = directory;
          if (
            result.errors.length ||
            options.interrupted() ||
            index === options.scenarios.length - 1
          )
            await close(result);
        }
        result.elapsedMs = Math.round(performance.now() - start);
        result.status = result.errors.length ? "failed" : "passed";
        await saveReport();
        log(
          `${result.status === "passed" ? "PASS" : "FAIL"} ${scenario.name} (${result.elapsedMs} ms)`,
        );
        for (const error of result.errors) log(`  ${error.phase}: ${error.message}`);
        if (result.errors.length) log(`  Artifacts: ${directory}`);
      }
    } finally {
      if (harness && lastResult) {
        await close(lastResult);
        await saveReport();
      }
    }
  }
  log(`\n${report.passed} passed, ${report.failed} failed, ${report.skipped} skipped.`);
  for (const test of tests.filter((test) => test.status === "failed")) {
    log(
      `FAIL run ${test.iteration}: ${test.name} [${test.errors.map((error) => error.phase).join(", ")}] — ${test.artifacts}`,
    );
  }
  log(`Report: ${path.join(options.artifacts, "result.json")}`);
  return report;
}
