import assert from "node:assert/strict";
import { spawn, execFileSync, type ChildProcess } from "node:child_process";
import * as fs from "node:fs/promises";
import path from "node:path";
import { createServer } from "node:net";
import { setTimeout as delay } from "node:timers/promises";

export class ObsHarness {
  snapshot: any;
  events: any[] = [];
  private child?: ChildProcess;
  private socket?: WebSocket;
  private commandId = 0;
  private failure?: Error;
  private closing = false;
  private base = "";
  private eventLog?: fs.FileHandle;
  private eventWrites: Promise<unknown> = Promise.resolve();
  private eventWriteError?: Error;
  private instanceId = "";
  private exit?: Promise<void>;
  private lastObservation: unknown = null;

  readonly root: string;
  readonly artifacts: string;
  readonly software: boolean;

  constructor(root: string, artifacts: string, software: boolean) {
    this.root = root;
    this.artifacts = artifacts;
    this.software = software;
  }

  async write(relative: string, contents: string) {
    const destination = path.join(this.artifacts, relative);
    await fs.mkdir(path.dirname(destination), { recursive: true });
    await fs.writeFile(destination, contents);
  }

  async waitFor<T>(
    label: string,
    predicate: () => T | Promise<T>,
    timeout = 20000,
    diagnostics?: { expected: unknown; observed: () => unknown },
  ): Promise<NonNullable<T>> {
    const deadline = performance.now() + timeout;
    let lastValue: unknown = "not sampled";
    while (performance.now() < deadline) {
      if (this.failure) throw this.failure;
      if (
        !this.closing &&
        this.child &&
        (this.child.exitCode !== null || this.child.signalCode !== null)
      ) {
        throw new Error(`OBS exited: ${this.child.exitCode ?? this.child.signalCode}`);
      }
      const value = await predicate();
      lastValue = value;
      if (value) return value as NonNullable<T>;
      await delay(50);
    }
    const details = {
      condition: label,
      timeoutMs: timeout,
      expected: diagnostics?.expected ?? label,
      observed: diagnostics
        ? diagnostics.observed()
        : {
            lastValue,
            lastResponse: this.lastObservation,
            match: this.snapshot?.match ?? null,
            monitor: this.snapshot?.monitor ?? null,
            replayBuffer: this.snapshot?.replayBuffer ?? null,
          },
    };
    const error = new Error(
      `Timed out after ${timeout} ms: ${label}\nExpected: ${JSON.stringify(details.expected)}\nObserved: ${JSON.stringify(details.observed)}\nArtifacts: ${this.artifacts}`,
    );
    throw Object.assign(error, { details });
  }

  async api(route: string, body?: unknown) {
    const response = await fetch(this.base + route, {
      method: body === undefined ? "GET" : "POST",
      headers: { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(3000),
    });
    const text = await response.text();
    assert(response.ok, `${route}: ${response.status} ${text}`);
    const result = text ? JSON.parse(text) : null;
    this.lastObservation = { route, response: result };
    return result;
  }

  abort() {
    this.failure = new Error("Test interrupted");
  }

  async command(payload: Record<string, unknown>) {
    const id = ++this.commandId;
    await this.write("command.tmp", JSON.stringify({ id, ...payload }));
    await fs.rename(
      path.join(this.artifacts, "command.tmp"),
      path.join(this.artifacts, "command.json"),
    );
    const response = await this.waitFor(`source command ${id}`, async () => {
      const response = JSON.parse(
        await fs.readFile(path.join(this.artifacts, "response.json"), "utf8"),
      );
      this.lastObservation = { command: payload, expectedId: id, response };
      if (response.id !== id) return false;
      assert.equal(response.error, "", `OBS source command ${id}`);
      return response;
    });
    await fs.appendFile(
      path.join(this.artifacts, "commands.jsonl"),
      JSON.stringify({ id, ...payload, response }) + "\n",
    );
    return response;
  }

  async fixture(name: string, relative: string, kind = "image_source") {
    const file = path.join(this.root, "frame_tests", relative);
    await fs.access(file);
    await this.command({ name, kind, path: file });
    await this.waitFor(`source ${name}`, async () => {
      const sources = await this.api("/api/v1/sources");
      return sources.some((source: any) => source.name === name && source.id === kind);
    });
    if (kind === "ffmpeg_source") {
      await this.waitFor("first decoded media frame", async () => {
        const status = await this.command({ action: "status", name });
        return status.width > 0 && status.height > 0;
      });
      await this.waitFor("media paused", async () => {
        const status = await this.command({ action: "pause", name });
        return status.paused;
      });
    }
  }

  async removeSource(name: string) {
    await this.command({ action: "remove", name });
    await this.waitFor(
      "source released",
      async () => !(await this.command({ action: "status", name })).exists,
    );
  }

  async ready() {
    await this.waitFor("session ready for the next scenario", async () => {
      const sources = await this.api("/api/v1/sources");
      return (
        !this.snapshot.monitor.enabled && !this.snapshot.replayBuffer.active && sources.length === 0
      );
    });
  }

  async startMonitor(sourceName: string) {
    assert(!this.snapshot.monitor.enabled);
    await this.api("/api/v1/monitor/start", { sourceName });
    await this.waitFor(
      "monitor and replay ready",
      () =>
        this.snapshot.monitor.enabled &&
        this.snapshot.monitor.sourceName === sourceName &&
        this.snapshot.replayBuffer.active,
    );
  }

  async stopMonitor() {
    await this.api("/api/v1/monitor/stop", {});
    await this.waitFor(
      "monitor and replay stopped",
      () => !this.snapshot.monitor.enabled && !this.snapshot.replayBuffer.active,
    );
  }

  async expectMatch(screen: string, mission: number, part: number, difficulty: number) {
    await this.waitFor(
      `${screen} ${mission}/${part}/${difficulty}`,
      () => {
        const match = this.snapshot?.match;
        return (
          match?.screen === screen &&
          match.mission === mission &&
          match.part === part &&
          match.difficulty === difficulty
        );
      },
      20000,
      {
        expected: { screen, mission, part, difficulty },
        observed: () => this.snapshot?.match ?? null,
      },
    );
  }

  async expectCapture(width: number, height: number) {
    const before = new Set(await fs.readdir(this.artifacts));
    await this.api("/api/v1/monitor/frame-dump", {
      enabled: true,
      source: this.snapshot.monitor.sourceName,
    });
    let directory = "";
    const errors: unknown[] = [];
    try {
      await this.waitFor("capture frames", async () => {
        directory =
          (await fs.readdir(this.artifacts)).find(
            (n) => n.startsWith("ge-frames-") && !before.has(n),
          ) ?? "";
        if (!directory) return false;
        return (await fs.readdir(path.join(this.artifacts, directory))).length >= 3;
      });
    } catch (error) {
      errors.push(error);
    } finally {
      try {
        await this.api("/api/v1/monitor/frame-dump", { enabled: false });
      } catch (error) {
        errors.push(error);
      }
    }
    if (errors.length === 1) throw errors[0];
    if (errors.length)
      throw new AggregateError(errors, "Capture failed and frame-dump cleanup failed");
    const files = await fs.readdir(path.join(this.artifacts, directory));
    assert(files.length >= 3);
    for (const file of files) {
      const bytes = await fs.readFile(path.join(this.artifacts, directory, file));
      assert.equal(bytes.subarray(0, 2).toString(), "BM");
      assert.equal(bytes.readUInt32LE(2), bytes.length, "complete BMP");
      assert.deepEqual([bytes.readInt32LE(18), Math.abs(bytes.readInt32LE(22))], [width, height]);
    }
  }

  async expectSavedRun(source: string, expected: Record<string, unknown>, eventStart: number) {
    const clip = await this.waitFor(
      `saved clip from ${source}`,
      async () => {
        const runs = await this.api("/api/v1/runs");
        const clips = runs.clips.filter((clip: any) => clip.metadata.sourceName === source);
        assert(clips.length <= 1, `Duplicate runs for ${source}`);
        const clip = clips[0];
        if (!clip?.path) return false;
        for (const [key, value] of Object.entries(expected))
          assert.equal(clip.metadata[key], value, key);
        return (
          this.events
            .slice(eventStart)
            .some((event) => event.type === "recordingSaved" && event.path === clip.path) && clip
        );
      },
      45000,
      {
        expected: { source, metadata: expected, clipFile: true, saveCompletionEvent: true },
        observed: () => ({
          lastResponse: this.lastObservation,
          replaySaves: this.snapshot?.replaySaves,
        }),
      },
    );
    assert(
      clip.path.startsWith(this.artifacts + path.sep),
      "clip is in the isolated test directory",
    );
    assert((await fs.stat(clip.path)).size > 0);
    const probe = JSON.parse(
      execFileSync(
        "ffprobe",
        [
          "-v",
          "error",
          "-show_entries",
          "format=duration:stream=codec_type,width,height",
          "-of",
          "json",
          clip.path,
        ],
        { encoding: "utf8", timeout: 10000 },
      ),
    );
    assert(Number(probe.format.duration) > 0, "playable clip duration");
    assert(
      probe.streams.some(
        (stream: any) => stream.codec_type === "video" && stream.width > 0 && stream.height > 0,
      ),
    );
    await this.write(`probe-${source}.json`, JSON.stringify(probe, null, 2));
    return clip;
  }

  async launch() {
    assert.equal(process.platform, "linux", "This harness requires Linux and Flatpak OBS");
    assert(process.env.DISPLAY, "DISPLAY is required. Use an X11 desktop or xvfb-run.");
    execFileSync("flatpak", ["info", "com.obsproject.Studio"], { stdio: "ignore", timeout: 10000 });
    execFileSync("ffprobe", ["-version"], { stdio: "ignore", timeout: 10000 });
    for (const file of [
      "the_golden_eye/bin/64bit/the_golden_eye.so",
      "the_golden_eye/bin/64bit/libgolden_core.so",
      "obs-run-data/the_golden_eye/cv_templates/en-colon.png",
    ]) {
      await fs.access(path.join(this.root, "obs2/build-flatpak", file));
    }
    // Let the OS select a port. The settings-path assertion rejects another server if a bind race occurs.
    const reservation = createServer();
    await new Promise<void>((resolve, reject) => {
      reservation.once("error", reject);
      reservation.listen(0, "127.0.0.1", resolve);
    });
    const port = (reservation.address() as { port: number }).port;
    await new Promise<void>((resolve) => reservation.close(() => resolve()));
    this.base = `http://127.0.0.1:${port}`;
    await this.configure();
    this.eventLog = await fs.open(path.join(this.artifacts, "events.jsonl"), "w");
    const log = await fs.open(path.join(this.artifacts, "obs.log"), "w");
    const config = path.join(this.artifacts, "config");
    this.child = spawn(
      "flatpak",
      [
        "run",
        "--instance-id-fd=3",
        "--device=dri",
        `--filesystem=${this.root}`,
        "--socket=x11",
        "--nosocket=wayland",
        "--nosocket=pulseaudio",
        `--env=TMPDIR=${this.artifacts}`,
        `--env=GE_OBS_TEST_DIR=${this.artifacts}`,
        `--env=GE_SERVER_PORT=${port}`,
        "--env=GE_DISABLE_BROWSER_DOCK=1",
        "--env=GE_UPDATE_CHECK_URL=http://127.0.0.1:1",
        `--env=OBS_PLUGINS_PATH=${this.root}/obs2/build-flatpak/%module%/bin/64bit`,
        `--env=OBS_PLUGINS_DATA_PATH=${this.root}/obs2/build-flatpak/obs-run-data`,
        "--env=LD_LIBRARY_PATH=/app/lib",
        "--env=QT_QPA_PLATFORM=xcb",
        ...(this.software ? ["--env=LIBGL_ALWAYS_SOFTWARE=1"] : []),
        "--command=env",
        "com.obsproject.Studio",
        `XDG_CONFIG_HOME=${config}`,
        `XDG_CACHE_HOME=${this.artifacts}/cache`,
        "/app/bin/obs",
        "--collection",
        "Tests",
        "--profile",
        "Tests",
        "--scene",
        "Fixture Scene",
        "--multi",
        "--disable-missing-files-check",
      ],
      { stdio: ["ignore", log.fd, log.fd, "pipe"], detached: true },
    );
    this.child.stdio[3]?.on("data", (data: Buffer) => {
      this.instanceId += data.toString();
    });
    this.child.on("error", (error) => {
      this.failure = error;
    });
    this.exit = new Promise((resolve) => this.child!.once("exit", () => resolve()));
    await log.close();
    await this.waitFor(
      "Lua control ready",
      () =>
        fs.access(path.join(this.artifacts, "response.json")).then(
          () => true,
          () => false,
        ),
      45000,
    );
    await this.waitFor("plugin ready", () =>
      this.api("/api/v1/sources").then(
        () => true,
        () => false,
      ),
    );
    this.socket = new WebSocket(this.base.replace("http:", "ws:") + "/api/v1/events/ws");
    this.socket.addEventListener("error", () => {
      if (!this.closing) this.failure = new Error("Plugin event connection failed");
    });
    this.socket.addEventListener("close", () => {
      if (!this.closing) this.failure = new Error("Plugin event connection closed");
    });
    this.socket.addEventListener("message", (message) => {
      if (this.closing) return;
      try {
        const event = JSON.parse(String(message.data));
        this.events.push(event);
        this.eventWrites = this.eventWrites
          .then(() => this.eventLog!.appendFile(JSON.stringify(event) + "\n"))
          .catch((error) => {
            this.eventWriteError = error;
            this.failure = error;
          });
        if (event.type === "snapshot") this.snapshot = event.state;
      } catch (error) {
        this.failure = error as Error;
      }
    });
    await this.waitFor("initial snapshot", () => this.snapshot);
    assert.equal(
      this.snapshot.settingsStatus.configPath,
      path.join(config, "the-golden-eye/settings.json"),
    );
    assert.equal(this.snapshot.settingsStatus.fileError, null);
    assert.deepEqual(this.snapshot.sources, [], "OBS must load only the empty test collection");
    const replay = await this.waitFor("OBS output initialization", async () => {
      const status = await this.api("/api/v1/replay-buffer/status");
      return status.available && status.outputDirectory && status;
    });
    assert.equal(replay.outputDirectory, path.join(this.artifacts, "replays"));
    if (this.software) {
      const log = await fs.readFile(path.join(this.artifacts, "obs.log"), "utf8");
      assert.match(
        log,
        /OpenGL on adapter.*(?:llvmpipe|softpipe)/i,
        "Mesa software renderer is active",
      );
    }
  }

  private async configure() {
    await this.write(
      "config/obs-studio/global.ini",
      "[General]\nFirstRun=true\nLastVersion=537001986\nEnableAutoUpdates=false\n",
    );
    await this.write(
      "config/obs-studio/user.ini",
      "[General]\nFirstRun=true\nConfirmOnExit=false\nPre19Defaults=false\nPre21Defaults=false\nPre23Defaults=false\n[Basic]\nProfile=Tests\nProfileDir=Tests\nSceneCollection=Tests\nSceneCollectionFile=Tests\nConfigOnNewProfile=false\n[BasicWindow]\nShowWhatsNew=false\n",
    );
    await this.write(
      "config/obs-studio/basic/profiles/Tests/basic.ini",
      `[General]\nName=Tests\n[Video]\nBaseCX=1440\nBaseCY=1080\nOutputCX=640\nOutputCY=480\nFPSType=0\nFPSCommon=30\n[Output]\nMode=Simple\n[SimpleOutput]\nRecRB=true\nRecRBTime=20\nRecRBSize=100\nRecEncoder=x264\nRecQuality=Small\nRecFormat2=mkv\nFilePath=${this.artifacts}/replays\n[Audio]\nSampleRate=48000\nChannelSetup=Stereo\n`,
    );
    await fs.mkdir(path.join(this.artifacts, "replays"));
    await this.write(
      "config/obs-studio/basic/scenes/Tests.json",
      JSON.stringify({
        name: "Tests",
        current_scene: "Fixture Scene",
        current_program_scene: "Fixture Scene",
        scene_order: [{ name: "Fixture Scene" }],
        sources: [{ name: "Fixture Scene", id: "scene", settings: { items: [] } }],
        modules: {
          "scripts-tool": [
            { path: path.join(this.root, "frame_tests/obs/control.lua"), settings: {} },
          ],
        },
      }),
    );
    await this.write(
      "config/the-golden-eye/settings.json",
      JSON.stringify({
        welcomeModalShown: true,
        discordNotificationsEnabled: false,
        autoUpdateEnabled: false,
        updateCheckInterval: "never",
        completedOutputPath: path.join(this.artifacts, "clips"),
        preRunPaddingSecs: 0,
        postRunPaddingSecs: 0,
        stopReplayBufferWhenMonitorStopped: true,
      }),
    );
  }

  async close() {
    this.closing = true;
    let shutdownError: unknown;
    if (this.child?.pid && this.child.exitCode === null && this.child.signalCode === null) {
      try {
        this.failure = undefined;
        await this.command({ action: "quit" });
        await Promise.race([this.exit, delay(15000, undefined, { ref: false })]);
        assert.equal(this.child.exitCode, 0, "OBS must exit cleanly");
        const log = await fs.readFile(path.join(this.artifacts, "obs.log"), "utf8");
        assert(log.includes("Freeing OBS context data"), "OBS completed native shutdown");
      } catch (error) {
        shutdownError = error;
        if (this.instanceId.trim()) {
          try {
            execFileSync("flatpak", ["kill", this.instanceId.trim()], {
              timeout: 5000,
              stdio: "ignore",
            });
          } catch {}
        }
        try {
          process.kill(-this.child.pid, "SIGKILL");
        } catch {}
        await Promise.race([this.exit, delay(5000, undefined, { ref: false })]);
      }
    }
    const errors: unknown[] = shutdownError ? [shutdownError] : [];
    try {
      this.socket?.close();
    } catch (error) {
      errors.push(error);
    }
    try {
      await this.eventWrites;
    } catch (error) {
      errors.push(error);
    }
    if (this.eventWriteError) errors.push(this.eventWriteError);
    try {
      await this.eventLog?.close();
    } catch (error) {
      errors.push(error);
    }
    if (errors.length === 1) throw errors[0];
    if (errors.length) throw new AggregateError(errors, "Multiple OBS cleanup failures");
  }
}
