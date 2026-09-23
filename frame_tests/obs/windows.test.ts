import assert from "node:assert/strict";
import * as fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { spawn } from "node:child_process";
import { ObsHarness } from "./harness.ts";
import { copyWindowsObs, readWindowsObsLog } from "./windows.ts";

test(
  "publishing the next command does not replace a file held open by Lua",
  { skip: process.platform !== "win32" },
  async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "obs-command-"));
    let reader: ReturnType<typeof spawn> | undefined;
    let readerExit: Promise<unknown> | undefined;
    try {
      const harness = new ObsHarness(directory, directory, false);
      for (const id of [1, 2])
        await fs.writeFile(
          path.join(directory, `response-${id}.json`),
          JSON.stringify({ id, error: "" }),
        );
      await harness.command({ action: "status" });
      const first = (await fs.readdir(directory)).find((file) => /^command.*\.json$/.test(file))!;
      reader = spawn(
        "powershell.exe",
        [
          "-NoProfile",
          "-Command",
          "$ErrorActionPreference = 'Stop'; $f = [IO.File]::Open($env:OBS_COMMAND_FILE, 'Open', 'Read', 'ReadWrite'); try { [Console]::WriteLine('locked'); [Console]::ReadLine() } finally { $f.Dispose() }",
        ],
        {
          env: { ...process.env, OBS_COMMAND_FILE: path.join(directory, first) },
          windowsHide: true,
        },
      );
      readerExit = new Promise((resolve) => reader!.once("close", resolve));
      await new Promise<void>((resolve, reject) => {
        reader!.once("error", reject);
        reader!.once("exit", () => reject(new Error("Command reader exited before locking")));
        reader!.stdout!.once("data", (data) => {
          if (data.toString().includes("locked")) resolve();
          else reject(new Error(`Unexpected command reader output: ${data}`));
        });
      });
      await harness.command({ action: "pause" });
      assert.equal(JSON.parse(await fs.readFile(path.join(directory, first), "utf8")).id, 1);
    } finally {
      reader?.stdin?.end("\n");
      await readerExit;
      await fs.rm(directory, { recursive: true, force: true });
    }
  },
);

test(
  "Windows cleanup terminates its release server without Unix process-group signals",
  { skip: process.platform !== "win32" },
  async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "obs-server-"));
    const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], {
      windowsHide: true,
    });
    try {
      await new Promise<void>((resolve, reject) => {
        child.once("spawn", resolve);
        child.once("error", reject);
      });
      const harness = new ObsHarness(directory, directory, false);
      Object.assign(harness, {
        releaseServer: child,
        releaseExit: new Promise<void>((resolve) => child.once("exit", () => resolve())),
      });
      await harness.close();
      assert(child.exitCode !== null || child.signalCode !== null, "release server exited");
    } finally {
      if (child.exitCode === null && child.signalCode === null) child.kill();
      await fs.rm(directory, { recursive: true, force: true });
    }
  },
);

test("portable OBS copy excludes personal configuration and installed Golden Eye binaries", async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "obs-copy-"));
  const source = path.join(directory, "installed");
  const target = path.join(directory, "test space 日本語");
  try {
    for (const file of [
      "bin/64bit/obs64.exe",
      "data/obs-studio/locale/en-US.ini",
      "obs-plugins/64bit/obs-scripting.dll",
      "obs-plugins/64bit/the_golden_eye.dll",
      "obs-plugins/64bit/golden_core.dll",
      "data/the_golden_eye/settings.json",
      "config/obs-studio/global.ini",
    ]) {
      await fs.mkdir(path.dirname(path.join(source, file)), { recursive: true });
      await fs.writeFile(path.join(source, file), "installed");
    }
    await copyWindowsObs(source, target);
    assert.equal(await fs.readFile(path.join(target, "bin/64bit/obs64.exe"), "utf8"), "installed");
    for (const file of [
      "config",
      "data/the_golden_eye",
      "obs-plugins/64bit/the_golden_eye.dll",
      "obs-plugins/64bit/golden_core.dll",
    ]) {
      await assert.rejects(fs.access(path.join(target, file)), { code: "ENOENT" });
      await fs.access(path.join(source, file));
    }
    await fs.writeFile(path.join(target, "bin/64bit/obs64.exe"), "changed");
    assert.equal(await fs.readFile(path.join(source, "bin/64bit/obs64.exe"), "utf8"), "installed");
    await assert.rejects(copyWindowsObs(source, path.join(source, "nested")), /outside/);
  } finally {
    await fs.rm(directory, { recursive: true, force: true });
  }
});

test("Windows reload checks read the current native OBS log after a restart", async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "obs-logs-"));
  try {
    assert.equal(await readWindowsObsLog(directory), "");
    const logs = path.join(directory, "obs-studio/logs");
    await fs.mkdir(logs, { recursive: true });
    await fs.writeFile(path.join(logs, "2026-09-23 10-00-00.txt"), "old reload succeeded");
    await fs.writeFile(path.join(logs, "2026-09-23 10-01-00.txt"), "new session");
    assert.equal(await readWindowsObsLog(directory), "new session");
  } finally {
    await fs.rm(directory, { recursive: true, force: true });
  }
});
