import type { Mat } from "@u4/opencv4nodejs";
import cv from "./opencv.ts";

import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { matchThreshold, imageScale } from "./common.ts";
import type { Screen } from "./matcher.ts";

const __dirname = dirname(fileURLToPath(import.meta.url));

let screen: Screen | null = null;
let image: Mat | null = null;

export type MatcherProcessMessage =
  | { type: "init"; filename: string; screen: Screen }
  | { type: "init-complete" }
  | {
      type: "match";
      buffer: Buffer;
      rows: number;
      cols: number;
      matType: number;
    }
  | { type: "match-complete"; maxVal: number; screen: Screen | null };

function send(message: MatcherProcessMessage) {
  process.send!(message);
}

process.on("message", async (data: MatcherProcessMessage) => {
  try {
    if (data.type === "init") {
      const { filename } = data;
      screen = data.screen;
      image = cv
        .imread(join(__dirname, "templates", `${filename}.png`))
        .rescale(imageScale)
        .cvtColor(cv.COLOR_BGR2GRAY);
      send({ type: "init-complete" });
    }

    if (data.type === "match" && image) {
      const { buffer, rows, cols, matType } = data;
      const sourceImage = new cv.Mat(buffer, rows, cols, matType);

      const result = sourceImage.matchTemplate(image, cv.TM_CCOEFF_NORMED);
      const { maxVal } = result.minMaxLoc();
      if (maxVal >= matchThreshold) {
        send({ type: "match-complete", maxVal, screen });
      } else {
        send({ type: "match-complete", maxVal, screen: null });
      }
    }
  } catch (err) {
    console.error("Error handling message:", err);
  }
});
