import cp from "node:child_process";
import { fileURLToPath } from "node:url";
import type { LevelInfo } from "./parse.ts";
import type { LlamaProcessMessage } from "./llama-process.ts";

export class LlamaProcess {
  process: cp.ChildProcess;
  initialised: Promise<void>;

  constructor() {
    this.process = cp.fork(
      fileURLToPath(new URL("./llama-process.ts", import.meta.url)),
      [],
      {
        serialization: "advanced",
      },
    );

    this.initialised = new Promise<void>((resolve, reject) => {
      this.process.once("message", (message: LlamaProcessMessage) => {
        if (message.type === "ready") {
          resolve();
          this.process.removeListener("error", reject);
        }
      });

      this.process.addListener("error", reject);
    });
  }

  send(message: LlamaProcessMessage) {
    this.process.send(message);
  }

  extractText(imageData: string): Promise<string> {
    this.send({ type: "extract-text", imageData });
    return new Promise((resolve) => {
      this.process.once("message", (message: LlamaProcessMessage) => {
        if (message.type === "extracted-text") {
          resolve(message.text);
        }
      });
    });
  }

  sendImage(imageData: string): Promise<LevelInfo> {
    this.send({ type: "extract-level-info", imageData });
    return new Promise((resolve) => {
      this.process.once("message", (message: LlamaProcessMessage) => {
        if (message.type === "level-info") {
          resolve(message.levelInfo);
        }
      });
    });
  }

  kill() {
    this.send({ type: "shutdown" });
    setTimeout(() => this.process.kill(), 100);
  }
}
