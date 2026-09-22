import assert from "node:assert/strict";
import * as fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { runSuite } from "./suite.ts";
import { ObsHarness } from "./harness.ts";

test("suite continues after launch, test, and cleanup failures and preserves every error", async () => {
  const artifacts = await fs.mkdtemp(path.join(os.tmpdir(), "obs-suite-"));
  const trace: string[] = [];
  let created = 0;
  try {
    const report = await runSuite({
      artifacts,
      repeats: 2,
      interrupted: () => false,
      log: () => {},
      create: () => {
        const id = ++created;
        return {
          async launch() {
            trace.push(`launch ${id}`);
            if (id === 1) throw new Error("launch failed");
          },
          async close() {
            trace.push(`close ${id}`);
            if (id === 2) throw new Error("cleanup failed");
          },
        };
      },
      scenarios: [
        {
          name: "startup",
          async run() {
            trace.push("startup body");
          },
        },
        {
          name: "assertion",
          async run() {
            throw new Error("test failed");
          },
        },
        {
          name: "following",
          async run() {
            trace.push("following body");
          },
        },
      ],
    });
    assert.deepEqual([report.passed, report.failed, report.skipped], [3, 3, 0]);
    assert.deepEqual(
      report.tests[1].errors.map((e) => [e.phase, e.message]),
      [
        ["test", "test failed"],
        ["cleanup", "cleanup failed"],
      ],
    );
    assert.equal(trace.filter((item) => item.startsWith("close")).length, 5);
    assert.equal(trace.filter((item) => item === "following body").length, 2);
    const saved = JSON.parse(await fs.readFile(path.join(artifacts, "result.json"), "utf8"));
    assert.deepEqual(saved, JSON.parse(JSON.stringify(report)));
    assert.equal(
      JSON.parse(await fs.readFile(path.join(artifacts, "case-2.json"), "utf8")).errors.length,
      2,
    );
  } finally {
    await fs.rm(artifacts, { recursive: true, force: true });
  }
});

test("interruption cleans the current scenario and skips all remaining scenarios", async () => {
  const artifacts = await fs.mkdtemp(path.join(os.tmpdir(), "obs-suite-"));
  let interrupted = false;
  let closed = 0;
  let created = 0;
  try {
    const report = await runSuite({
      artifacts,
      repeats: 2,
      interrupted: () => interrupted,
      log: () => {},
      create: () => {
        created++;
        return {
          async launch() {},
          async close() {
            closed++;
          },
        };
      },
      scenarios: [
        {
          name: "interrupt",
          async run() {
            interrupted = true;
          },
        },
        {
          name: "never run",
          async run() {
            assert.fail("must skip");
          },
        },
      ],
    });
    assert.deepEqual([created, closed, report.failed, report.skipped], [1, 1, 1, 3]);
    assert.match(report.tests[0].errors[0].message, /interrupted/);
  } finally {
    await fs.rm(artifacts, { recursive: true, force: true });
  }
});

test("timeouts print and retain expected and last observed values", async () => {
  const h = new ObsHarness("/tmp", "/tmp/obs-diagnostics", false);
  const expected = { screen: "Start", mission: 1 };
  const observed = { screen: "Unknown", mission: -1 };
  await assert.rejects(
    h.waitFor("mission match", () => false, 5, { expected, observed: () => observed }),
    (error) => {
      assert(error instanceof Error && "details" in error);
      assert.match(error.message, /Expected:.*Start/);
      assert.match(error.message, /Observed:.*Unknown/);
      assert.deepEqual((error.details as any).expected, expected);
      assert.deepEqual((error.details as any).observed, observed);
      return true;
    },
  );
});

test("passing scenarios share a session, repetitions reset it, and final cleanup errors fail", async () => {
  const artifacts = await fs.mkdtemp(path.join(os.tmpdir(), "obs-suite-"));
  const used: number[] = [];
  let created = 0;
  let closed = 0;
  try {
    const report = await runSuite({
      artifacts,
      repeats: 2,
      interrupted: () => false,
      log: () => {},
      create: () => ({
        id: ++created,
        async launch() {},
        async ready() {},
        async close() {
          closed++;
          throw new Error("shutdown failed");
        },
      }),
      scenarios: ["first", "second"].map((name) => ({
        name,
        async run(h: { id: number }) {
          used.push(h.id);
        },
      })),
    });
    assert.deepEqual(used, [1, 1, 2, 2]);
    assert.equal(closed, 2);
    assert.deepEqual([report.passed, report.failed], [2, 2]);
    assert.equal(report.tests[1].errors[0].phase, "cleanup");
    assert.equal(report.tests[0].artifacts, report.tests[1].artifacts);
    assert.notEqual(report.tests[1].artifacts, report.tests[2].artifacts);
  } finally {
    await fs.rm(artifacts, { recursive: true, force: true });
  }
});

test("a failed readiness check closes the session before the next scenario", async () => {
  const artifacts = await fs.mkdtemp(path.join(os.tmpdir(), "obs-suite-"));
  const trace: string[] = [];
  let created = 0;
  try {
    const report = await runSuite({
      artifacts,
      repeats: 1,
      interrupted: () => false,
      log: () => {},
      create: () => {
        const id = ++created;
        return {
          async launch() {
            trace.push(`launch ${id}`);
          },
          async ready() {
            if (id === 1) throw new Error("source remains");
          },
          async close() {
            trace.push(`close ${id}`);
          },
        };
      },
      scenarios: ["first", "second"].map((name) => ({ name, async run() {} })),
    });
    assert.deepEqual(trace, ["launch 1", "close 1", "launch 2", "close 2"]);
    assert.deepEqual([report.passed, report.failed], [1, 1]);
  } finally {
    await fs.rm(artifacts, { recursive: true, force: true });
  }
});

test("cleanup rejects an OBS exit between the last assertion and shutdown", async () => {
  for (const [exitCode, signalCode] of [
    [0, null],
    [1, null],
    [null, "SIGSEGV"],
  ] as const) {
    const harness = new ObsHarness("/unused", "/unused", false);
    Object.assign(harness, { child: { exitCode, signalCode } });
    await assert.rejects(harness.close(), /OBS exited before cleanup/);
  }
});
