import * as path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rustRoot = path.join(__dirname, "..", "obs2", "rust");

export interface Runner {
  name: string;
  command: (screenshotPath: string, lang: string) => string;
}

export const runners: Runner[] = [
  {
    name: "test_match.ts (cv templates)",
    command: (sp, lang) => `"${rustRoot}/target/release/test_match" "${lang}" "${sp}"`,
  },
];
