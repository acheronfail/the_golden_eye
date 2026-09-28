import assert from "node:assert/strict";
import * as fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { createServer, type RequestListener } from "node:http";
import type { AddressInfo } from "node:net";
import { runSuite } from "./suite.ts";
import { ObsHarness } from "./harness.ts";

async function withApi(handler: RequestListener, run: (h: ObsHarness) => Promise<void>) {
  const server = createServer(handler);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const h = new ObsHarness("/tmp", "/tmp/obs-diagnostics", false);
  Object.assign(h, { base: `http://127.0.0.1:${(server.address() as AddressInfo).port}` });
  try {
    await run(h);
  } finally {
    server.closeAllConnections();
    await new Promise<void>((resolve, reject) =>
      server.close((error) => (error ? reject(error) : resolve())),
    );
  }
}

for (const partialBody of [false, true]) {
  test(`polling recovers after an OBS read timeout (${partialBody ? "body" : "headers"})`, async () => {
    let requests = 0;
    await withApi(
      (_request, response) => {
        if (++requests === 1) {
          if (partialBody) response.write('{"ready":');
          return;
        }
        response.end('{"ready":true}');
      },
      async (h) => {
        let polls = 0;
        const result = await h.waitFor(
          "OBS responsive",
          (remainingMs) =>
            h.api(
              "/api/v1/runs",
              undefined,
              "GET",
              undefined,
              Math.min(++polls === 1 ? 100 : 3000, remainingMs),
            ),
          10000,
        );
        assert.deepEqual(result, { ready: true });
        assert(requests >= 2, "the unanswered read must be retried");
      },
    );
  });
}

test(
  "persistent read stalls exhaust the condition deadline and retain request diagnostics",
  { timeout: 10000 },
  async () => {
    await withApi(
      () => {},
      async (h) => {
        await assert.rejects(
          h.waitFor(
            "saved clip",
            (remainingMs) =>
              h.api("/api/v1/runs", undefined, "GET", undefined, Math.min(100, remainingMs)),
            300,
          ),
          (error: any) => {
            assert.match(error.message, /Timed out after 300 ms: saved clip/);
            assert(error.details.requestTimeouts >= 1);
            assert.equal(error.details.lastRequestTimeout.route, "/api/v1/runs");
            assert.equal(error.details.lastRequestTimeout.method, "GET");
            assert(error.details.lastRequestTimeout.timeoutMs <= 100);
            return true;
          },
        );
      },
    );
  },
);

test("polling never retries timed-out mutations", async () => {
  let attempts = 0;
  await withApi(
    () => {},
    async (h) => {
      await assert.rejects(
        h.waitFor("start monitor", () => {
          attempts++;
          return h.api("/api/v1/monitor/start", {}, "POST", undefined, 100);
        }),
        /POST .*monitor\/start timed out/,
      );
      assert.equal(attempts, 1);
    },
  );
});

test("OBS failure during a timed-out read prevents another polling attempt", async () => {
  await withApi(
    () => {},
    async (h) => {
      let attempts = 0;
      await assert.rejects(
        h.waitFor("saved clip", async () => {
          attempts++;
          try {
            return await h.api("/api/v1/runs", undefined, "GET", undefined, 100);
          } finally {
            Object.assign(h, { failure: new Error("Plugin event connection closed") });
          }
        }),
        /Plugin event connection closed/,
      );
      assert.equal(attempts, 1);
    },
  );
});

test("HTTP failures and transport disconnects still fail polling immediately", async () => {
  for (const disconnect of [false, true]) {
    let requests = 0;
    await withApi(
      (request, response) => {
        requests++;
        if (disconnect) request.socket.destroy();
        else {
          response.statusCode = 500;
          response.end("broken");
        }
      },
      async (h) => {
        await assert.rejects(
          h.waitFor("saved clip", () => h.api("/api/v1/runs")),
          disconnect ? /fetch failed/ : /500 broken/,
        );
        assert.equal(requests, 1);
      },
    );
  }
});

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
