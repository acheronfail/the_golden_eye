import assert from "node:assert/strict";
import * as fs from "node:fs/promises";
import path from "node:path";
import { createServer } from "node:net";
import { fileURLToPath } from "node:url";
import { ObsHarness } from "./harness.ts";
import { playRun } from "./media.ts";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const directory = await fs.mkdtemp(path.join(root, "obs2/build/real-obs-browser-"));
const reservation = createServer();
await new Promise<void>((resolve) => reservation.listen(0, "127.0.0.1", resolve));
const debugPort = (reservation.address() as { port: number }).port;
await new Promise<void>((resolve) => reservation.close(() => resolve()));
const h = new ObsHarness(root, directory, true, { browserDock: true, browserDebugPort: debugPort });
let socket: WebSocket | undefined;
let nextId = 0;
const pending = new Map<
  number,
  { resolve: (value: any) => void; reject: (error: Error) => void }
>();
const exceptions: unknown[] = [];
function send(method: string, params: object = {}): Promise<any> {
  const id = ++nextId;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`CEF command timed out: ${method}`));
    }, 10000);
    pending.set(id, {
      resolve: (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      reject: (error) => {
        clearTimeout(timer);
        reject(error);
      },
    });
    socket!.send(JSON.stringify({ id, method, params }));
  });
}
async function body() {
  const result = await send("Runtime.evaluate", {
    expression: "document.body.textContent",
    returnByValue: true,
  });
  return result.result.value as string;
}
async function capture(name: string) {
  const { data } = await send("Page.captureScreenshot");
  await fs.writeFile(path.join(directory, `${name}.png`), Buffer.from(data, "base64"));
  await fs.writeFile(path.join(directory, `${name}.txt`), await body());
}
console.log(`Browser dock artifacts: ${directory}`);
let testError: unknown;
try {
  await h.launch();
  const obsLog = await fs.readFile(path.join(directory, "obs.log"), "utf8");
  if (/OBS (?:3[3-9]|[4-9]\d)\./.test(obsLog)) {
    assert.match(obsLog, /browser initialized before dock registration/);
  }
  const target = await h.waitFor("embedded CEF dock target", async () => {
    try {
      const targets = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
      return targets.find(
        (target: any) => target.type === "page" && /^http:\/\/127\.0\.0\.1:\d+\/$/.test(target.url),
      );
    } catch {
      return undefined;
    }
  });
  socket = new WebSocket(target.webSocketDebuggerUrl);
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(String(event.data));
    if (message.method === "Runtime.exceptionThrown") exceptions.push(message.params);
    const request = pending.get(message.id);
    if (request) {
      pending.delete(message.id);
      if (message.error) request.reject(new Error(JSON.stringify(message.error)));
      else request.resolve(message.result);
    }
  });
  await new Promise<void>((resolve, reject) => {
    socket!.onopen = () => resolve();
    socket!.onerror = reject;
  });
  await send("Runtime.enable");
  await send("Emulation.setDeviceMetricsOverride", {
    width: 640,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await h.waitFor("Svelte dashboard rendered inside CEF", async () =>
    /Monitoring/.test(await body()),
  );
  await h.fixture(
    "Dock source before reload",
    "screenshots-emu/en - start - 01 - Secret Agent.png",
  );
  await h.waitFor("embedded dashboard receives source snapshot", async () =>
    (await body()).includes("Dock source before reload"),
  );
  await capture("before-reload");
  await h.removeSource("Dock source before reload");
  await h.updateMode("valid");
  await h.api("/api/v1/updates/download", {});
  await h.reload(() => h.api("/api/v1/updates/apply", {}), "core reload succeeded");
  await h.fixture("Dock source after reload", "screenshots-emu/en - start - 01 - Secret Agent.png");
  await h.waitFor("embedded dashboard reconnects after core reload", async () =>
    (await body()).includes("Dock source after reload"),
  );
  await capture("after-reload");
  await h.removeSource("Dock source after reload");
  await playRun(h, "Embedded dock replay", "rt4kce-completed.mp4", 28, "complete");
  assert.deepEqual(exceptions, [], "Embedded dashboard has no uncaught JavaScript exceptions");
  await fs.writeFile(
    path.join(directory, "browser.json"),
    JSON.stringify({ passed: true, target: target.url, exceptions }, null, 2),
  );
  console.log("PASS embedded CEF dock renders and reconnects after core reload");
} catch (error) {
  testError = error;
  await fs.writeFile(
    path.join(directory, "browser.json"),
    JSON.stringify({ passed: false, error: String(error), exceptions }, null, 2),
  );
  throw error;
} finally {
  socket?.close();
  try {
    await h.close();
  } catch (error) {
    if (testError) throw new AggregateError([testError, error], "Browser test and cleanup failed");
    throw error;
  }
}
