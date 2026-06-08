import * as path from "node:path";
import * as fsp from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { NumberLevelMap, type Level } from "./levels.ts";
import { Difficulties, type Difficulty } from "./difficulty.ts";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export const Screens = ["007opts", "abort", "complete", "kia", "failed", "levels", "select", "start", "stats"] as const;
export type Screen = (typeof Screens)[number];

export interface ScreenshotInfo {
  name: string;
  lang: string;
  screen: Screen;
  level?: Level;
  difficulty?: Difficulty;
  extra: string[];
  filePath: string;
}

export const getScreenshots = async () => {
  const screenshotDir = path.join(__dirname, "./screenshots");
  return await fsp.readdir(screenshotDir).then((entries) =>
    entries.map((entry): ScreenshotInfo => {
      const filePath = path.join(screenshotDir, entry);
      const name = path.basename(entry, ".png");

      const [lang, screenStr, levelNumStr, difficultyStr, ...extra] = name.split(" - ");

      const screen = Screens.find((s) => s === screenStr);
      if (!screen) {
        throw new Error(`Invalid screen name in filename: ${entry}`);
      }

      if (screen === "levels") {
        return { name, lang, screen, extra, filePath };
      }

      const level = NumberLevelMap.get(parseInt(levelNumStr, 10));
      if (!level) {
        throw new Error(`Invalid level number in filename: ${entry}`);
      }

      if (screen === "select") {
        return { name, lang, screen, level, extra, filePath };
      }

      const difficulty = Difficulties.find((d) => d === difficultyStr);
      if (!difficulty) {
        throw new Error(`Invalid difficulty in filename: ${entry}`);
      }

      return {
        name,
        lang,
        screen,
        level,
        difficulty,
        extra,
        filePath,
      };
    }),
  );
};
