import { extractLevelInfo, type LevelInfo } from "./parse.ts";

import { fileURLToPath } from "node:url";
import path from "node:path";
import cp from "node:child_process";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const modelPath = path.join(__dirname, "..", "models/gemma-4-E2B-it-Q4_K_M.gguf");
const mmprojPath = path.join(__dirname, "..", "models/mmproj-BF16.gguf");
const modelName = path.basename(modelPath, ".gguf");

export type LlamaProcessMessage =
  | { type: "ready" }
  | { type: "shutdown" }
  | { type: "extract-level-info"; imageData: string }
  | { type: "level-info"; levelInfo: LevelInfo; llamaResult: LlamaParseResult }
  | { type: "extract-text"; imageData: string }
  | { type: "extracted-text"; result: LlamaParseResult };

function send(message: LlamaProcessMessage) {
  process.send!(message);
}

export interface LlamaParseResult {
  text: string;
  difficulty: string;
  missionNumber: number;
  partNumber: string;
}

export class LlamaWrapper {
  server: cp.ChildProcessWithoutNullStreams;

  constructor() {
    this.server = cp.spawn("./llama/llama-server", [
      ...[`--model`, `${modelPath}`],
      ...[`--mmproj`, `${mmprojPath}`],
      ...["--ctx-size", "1024"],
      ...["--port", "1234"],
      ...["--host", "localhost"],
      ...["--temperature", "0.0"],
      ...["--repeat-penalty", "1.2"],
      ...["--reasoning", "off"],
      ...(process.env.LLAMA_EXTRA_ARGS ? process.env.LLAMA_EXTRA_ARGS.split(" ") : []),
    ]);

    this.server.stdout.pipe(process.stdout);
    if (process.env.DEBUG) {
      this.server.stderr.pipe(process.stderr);
    }

    const killServer = () => {
      this.server.kill();
    };

    process.on("exit", killServer);
    process.on("SIGTERM", () => {
      killServer();
      process.exit(1);
    });
    process.on("SIGINT", () => {
      killServer();
      process.exit(1);
    });

    this.server.stderr.on("data", (data) => {
      if (data.toString().includes("server is listening")) {
        send({ type: "ready" });
      }
    });
  }

  async extractText(imageDataUrl: string): Promise<LlamaParseResult> {
    const isJapanese = process.env.GE_LANG?.includes("jp");
    const res = await fetch("http://localhost:1234/v1/chat/completions", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        model: modelName,
        tool_choice: "required",
        tools: [
          isJapanese
            ? {
                type: "function",
                function: {
                  name: "extract_text",
                  description:
                    'Extract Japanese text and numbers from an image. The "text" field is all the text on screen, the "difficulty" field is the difficulty of the level which is one of "スパイ", "特命スパイ", "<00Agent>" or "007" and is on the first line before the ":", the "missionNumber" field is the number directly after "ミッション" near the start and is [0-9], and "partNumber" is the string of roman numerals after "パート" in the text.',
                  parameters: {
                    type: "object",
                    properties: {
                      text: {
                        type: "string",
                        description: "The Japanese text and numbers extracted from the image.",
                      },
                      difficulty: {
                        type: "string",
                        description:
                          'The difficulty of the level, either "スパイ", "特命スパイ", "<00Agent>" or "007".',
                      },
                      missionNumber: {
                        type: "number",
                        description: 'The number after "ミッション" in the text.',
                      },
                      partNumber: {
                        type: "string",
                        description: 'The roman numerals after "パート" in the text, as a lowercase string (e.g. "iii").',
                      },
                    },
                    required: ["text"],
                  },
                },
              }
            : {
                type: "function",
                function: {
                  name: "extract_text",
                  description:
                    'Extract text from an image. The "text" field is all the text, the "difficulty" field is the difficulty of the level which is near the start, the "missionNumber" field is the number after "mission" near the start, and "partNumber" is the string of roman numerals after "part" in the text.',
                  parameters: {
                    type: "object",
                    properties: {
                      text: {
                        type: "string",
                        description: "The text extracted from the image.",
                      },
                      difficulty: {
                        type: "string",
                        description:
                          'The difficulty of the level, either "Agent", "Secret Agent", "00 Agent" or "007".',
                      },
                      missionNumber: {
                        type: "number",
                        description: 'The number after "mission" in the text.',
                      },
                      partNumber: {
                        type: "string",
                        description: 'The roman numerals after "part" in the text, as a lowercase string (e.g. "iii").',
                      },
                    },
                    required: ["text"],
                  },
                },
              },
        ],
        messages: [
          {
            role: "system",
            content: "You are an OCR program that outputs the text in an image in plain text with nothing else.",
          },
          {
            role: "user",
            content: [
              {
                type: "image_url",
                image_url: {
                  url: imageDataUrl,
                },
              },
              {
                type: "text",
                text: "Extract the text from this image including the time strings /\\d+:\\d+/, and call the extract_text tool. Remember that there is always a number 0-9 following the word 'mission' or 'ミッション', and there is always some roman numbers [ivxl] following the word 'part' or 'パート'; make sure that both the mission number and part number appear in the extracted text.",
              },
            ],
          },
        ],
      }),
    });

    if (!res.ok) {
      throw new Error(`Request failed with status ${res.status}: ${await res.text()}`);
    }

    const data = await res.json();
    const result = JSON.parse(data.choices[0].message.tool_calls[0].function.arguments) as LlamaParseResult;
    return result;
  }

  kill() {
    this.server.kill();
  }
}

//
// Main
//

const llama = new LlamaWrapper();

process.on("message", async (data: LlamaProcessMessage) => {
  try {
    const { type } = data;
    if (type === "shutdown") {
      llama.kill();
      process.exit(0);
    }

    if (type === "extract-level-info") {
      const { imageData } = data;
      const result = await llama.extractText(imageData);
      const isJapanese = process.env.GE_LANG?.includes("jp") ?? false;
      const { levelInfo, llamaResult } = extractLevelInfo(result, isJapanese);
      send({ type: "level-info", levelInfo, llamaResult });
    }
    if (type === "extract-text") {
      const { imageData } = data;
      const result = await llama.extractText(imageData);
      send({ type: "extracted-text", result });
    }
  } catch (err) {
    console.error("Error handling message:", err);
  }
});
