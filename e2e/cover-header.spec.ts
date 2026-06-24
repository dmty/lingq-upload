import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const PROJECT_KEY = "cover-fixture";

function fixtureScript(): string {
  const chapters = Array.from({ length: 4 }, (_, i) => ({
    id: `idx:${i}`, order: i, title: `Chapter ${i + 1}`, body: "x".repeat(50), kind: "body",
  }));
  const mapping = {
    pairs: chapters.map((c, i) => ({
      chapter_id: c.id, track_id: i < 2 ? "t0" : "t1",
      confidence: 1, touched: false, original_confidence: 1,
    })),
    parking_lot: [], op_id: 0,
    buckets: [
      { trackId: "t0", atomTitle: "Audio 1", atomDurationSec: 600, charsPerSec: 5, audioPath: "/audio/t0.m4a", window: [0, 600] },
      { trackId: "t1", atomTitle: "Audio 2", atomDurationSec: 300, charsPerSec: 5, audioPath: "/audio/t1.m4a", window: [0, 300] },
    ],
  };
  const inspection = {
    title: "Botchan", chapter_count: 4, track_count: 2,
    condition: "many_to_few" as const,
    options: ["split_proportional", "cancel"] as const,
    preselect: "split_proportional" as const, bucket_preview: null,
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = ${JSON.stringify(inspection)};
    window.__mappingState__.seed(${JSON.stringify(PROJECT_KEY)}, ${JSON.stringify(mapping)});
    window.__projectMeta__ = window.__projectMeta__ || {};
    window.__projectMeta__[${JSON.stringify(PROJECT_KEY)}] = {
      title: "Botchan", authors: ["Natsume Soseki"], cover_path: null,
    };
    window.__dialogPickPath__ = "/picked/botchan-cover.png";
  })();`;
}

test.describe("match cover header", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("shows cover, title, author, and an Add-cover control", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("match-title")).toHaveText("Botchan");
    await expect(page.getByTestId("match-author")).toHaveText("Natsume Soseki");
    await expect(page.getByTestId("match-cover")).toBeVisible();
    // cover_path is null → no real <img>, fallback tile, button reads "Add cover".
    await expect(page.getByTestId("cover-replace")).toHaveText("Add cover");
  });

  test("replace picks a file and flips the control to Replace cover", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("cover-replace")).toHaveText("Add cover");
    await page.getByTestId("cover-replace").click();
    // open() returns __dialogPickPath__ → cmdSetCover → coverPath state updates.
    await expect(page.getByTestId("cover-replace")).toHaveText("Replace cover");
  });
});
