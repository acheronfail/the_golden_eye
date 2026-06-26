import * as path from "node:path";
import * as fsp from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { NumberLevelMap, type Level } from "./levels.ts";
import { Difficulties, type Difficulty } from "./difficulty.ts";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export const Screens = ["007opts", "abort", "complete", "kia", "failed", "levels", "select", "start", "stats"] as const;
export type Screen = (typeof Screens)[number];

export interface ScreenshotInfo {
  tag: string;
  name: string;
  lang: string;
  screen: Screen;
  level?: Level;
  difficulty?: Difficulty;
  extra: string[];
  filePath: string;
}

export const getScreenshots = async () => {
  const screenshotDirs = await fsp
    .readdir(__dirname)
    .then((entries) =>
      Promise.all(
        entries
          .filter((entry) => entry.startsWith("screenshots"))
          .map(async (entry) => {
            const fullPath = path.join(__dirname, entry);
            const isDir = await fsp.stat(fullPath).then(
              (s) => s.isDirectory(),
              () => false,
            );
            return { fullPath, isDir };
          }),
      ),
    )
    .then((paths) => paths.filter((p) => p.isDir).map((p) => p.fullPath));

  const allScreenshotPaths = await Promise.all(
    screenshotDirs.map((dir) => fsp.readdir(dir).then((entries) => entries.map((entry) => path.join(dir, entry)))),
  );

  return allScreenshotPaths.flat().map((filePath): ScreenshotInfo => {
    const entry = path.basename(filePath);
    const name = path.basename(entry, ".png");
    const tag = path.basename(path.dirname(filePath)).replace('screenshots-', '');

    const [lang, screenStr, levelNumStr, difficultyStr, ...extra] = name.split(" - ");

    const screen = Screens.find((s) => s === screenStr);
    if (!screen) {
      throw new Error(`Invalid screen name in filename: ${entry}`);
    }

    if (screen === "levels") {
      return { tag, name, lang, screen, extra, filePath };
    }

    const level = NumberLevelMap.get(parseInt(levelNumStr, 10));
    if (!level) {
      throw new Error(`Invalid level number in filename: ${entry}`);
    }

    if (screen === "select") {
      return { tag, name, lang, screen, level, extra, filePath };
    }

    const difficulty = Difficulties.find((d) => d === difficultyStr);
    if (!difficulty) {
      throw new Error(`Invalid difficulty in filename: ${entry}`);
    }

    return {
      tag,
      name,
      lang,
      screen,
      level,
      difficulty,
      extra,
      filePath,
    };
  });
};
