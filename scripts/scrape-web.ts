import fsp from "node:fs/promises";
import { chromium } from "playwright";

const base = "https://rankings.the-elite.net";

const data: {
  stage: string;
  systems: Record<string, number>[];
}[] = [];
const difficulties = ["Agent", "Secret Agent", "00 Agent"];

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

try {
  await page.goto(`${base}/goldeneye`, { waitUntil: "domcontentloaded" });
  await page.waitForSelector("#wr-table a.stage-title");

  const stages = await page.$$eval("#wr-table a.stage-title", (links) => {
    return links
      .map((link) => {
        const title = link.textContent?.trim() ?? "";
        const href = link.getAttribute("href") ?? "";
        return { title, href };
      })
      .filter((stage) => stage.title && stage.href);
  });

  for (const stage of stages) {
    console.log(`Processing stage: ${stage.title}...`);
    await page.goto(new URL(stage.href, base).toString(), {
      waitUntil: "domcontentloaded",
    });
    await page.waitForSelector(".stage-table .rank");

    const links = await page.$$eval(
      ".stage-table",
      (tables, levels) => {
        const out: string[][] = [[], [], []];

        for (let i = 0; i < levels.length; i++) {
          const table = tables[i];
          if (!table) {
            continue;
          }

          const rankCell = Array.from(table.querySelectorAll(".rank")).filter(
            (el) => el.textContent?.trim() === "1",
          );
          for (const cell of rankCell) {
            const row = cell.closest("tr");
            const timeHref = row?.querySelector(".time")?.getAttribute("href");
            if (timeHref) {
              out[i].push(timeHref);
            }
          }
        }

        return out;
      },
      difficulties,
    );

    console.log(
      `Found ${links.reduce((sum, group) => sum + group.length, 0)} WR links for stage: ${stage.title}...`,
    );

    const systemCountsPerDifficulty: Record<string, number>[] =
      difficulties.map(() => ({}));
    for (let i = 0; i < difficulties.length; i++) {
      const linkGroup = links[i];
      for (const timeLink of linkGroup) {
        await page.goto(new URL(timeLink, base).toString(), {
          waitUntil: "domcontentloaded",
        });
        await page.waitForSelector("ul#time-details");
        const liElements = await page.$$("ul#time-details li");
        for (const li of liElements) {
          const text = await li.textContent();
          if (text?.includes("System:")) {
            const system = text.split("System:")[1].trim();
            systemCountsPerDifficulty[i][system] =
              (systemCountsPerDifficulty[i][system] || 0) + 1;
          }
        }
      }
    }

    data.push({ stage: stage.title, systems: systemCountsPerDifficulty });
  }

  console.log(JSON.stringify(data, null, 2));
  await fsp.writeFile("goldeneye.json", JSON.stringify(data, null, 2));
} finally {
  await browser.close();
}
