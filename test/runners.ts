import * as path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rustRoot = path.join(__dirname, "..", "obs2", "rust");

export interface Runner {
  name: string;
  build?: (debug: boolean) => string;
  command: (debug: boolean, screenshotPath: string) => string;
}

export const runners: Runner[] = [
  {
    name: "cv templates",
    build: (debug) => `cd "${rustRoot}" && cargo build --bin test_match ${debug ? "" : "--release"}`,
    command: (debug, sp) => `"${rustRoot}/target/${debug ? "debug" : "release"}/test_match" "${sp}"`,
  },
];
