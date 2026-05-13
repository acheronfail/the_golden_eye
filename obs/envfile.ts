import fs from "node:fs/promises";

export async function readEnv() {
  const file = await fs.readFile(".env", "utf-8");
  const lines = file.split("\n");
  for (const line of lines) {
    if (line.trim().startsWith("#") || !line.includes("=")) {
      continue; // Skip comments and invalid lines
    }

    const splitIndex = line.indexOf("=");
    const key = line.slice(0, splitIndex).trim();
    const value = line.slice(splitIndex + 1).trim();
    if (key && value) {
      process.env[key] = value;
    }
  }
}
