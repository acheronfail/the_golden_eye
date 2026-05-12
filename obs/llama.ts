import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs/promises';
import cp from 'node:child_process';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const modelPath = path.join(__dirname, '..', 'models/gemma-4-E2B-it-Q4_K_M.gguf');
const mmprojPath = path.join(__dirname, '..', 'models/mmproj-BF16.gguf');

const modelName = path.basename(modelPath, '.gguf');

export class Llama {
  server: cp.ChildProcessWithoutNullStreams;
  initialised: Promise<void>;

  constructor() {
    this.server = cp.spawn('./llama/llama-server', [
      ...[`--model`, `${modelPath}`],
      ...[`--mmproj`, `${mmprojPath}`],
      ...['--ctx-size', '1024'],
      ...['--port', '1234'],
      ...['--host', 'localhost'],
      ...['--temperature', '0.0'],
      ...['--repeat-penalty', '1.2'],
      ...['--reasoning', 'off'],
      ...(process.env.LLAMA_EXTRA_ARGS ? process.env.LLAMA_EXTRA_ARGS.split(' ') : []),
    ]);

    this.server.stdout.pipe(process.stdout);
    if (process.env.DEBUG) {
      this.server.stderr.pipe(process.stderr);
    }

    const killServer = () => {
      this.server.kill();
    };

    process.on('exit', killServer);
    process.on('SIGTERM', () => { killServer(); process.exit(1); });
    process.on('SIGINT', () => { killServer(); process.exit(1); });

    this.initialised = new Promise((resolve, reject) => {
      this.server.stderr.on('data', (data) => {
        if (data.toString().includes('server is listening')) {
          resolve();
        }
      });
    });
  }

  async extractText(imageDataUrl: string): Promise<string> {
    const res = await fetch('http://localhost:1234/v1/chat/completions', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        model: modelName,
        tool_choice: 'required',
        tools: [
          {
            type: 'function',
            function: {
              name: 'extract_text',
              description: 'Extract text from an image.',
              parameters: {
                type: 'object',
                properties: {
                  text: {
                    type: 'string',
                    description: 'The text extracted from the image.',
                  },
                },
                required: ['text'],
              },
            },
          },
        ],
        messages: [
          {
            role: 'system',
            content: 'You are an OCR program that outputs the text in an image in plain text with nothing else.',
          },
          {
            role: 'user',
            content: [
              {
                type: 'image_url',
                image_url: {
                  url: imageDataUrl,
                },
              },
              {
                type: 'text',
                text: 'Extract the text from this image, and call the extract_text tool.',
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
    const result = JSON.parse(data.choices[0].message.tool_calls[0].function.arguments);
    return result.text;
  }

  kill() {
    this.server.kill();
  }
}
