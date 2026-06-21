import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const PROJECT_KEY = "bucket-fixture";

function fixtureScript(): string {
  const chapters = Array.from({ length: 5 }, (_, i) => ({
    id: `idx:${i}`,
    order: i,
    title: `Chapter ${i + 1}`,
    body: "x".repeat(100),
    kind: "body",
  }));
  const mapping = {
    pairs: [
      { chapter_id: "idx:0", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:1", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:2", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:3", track_id: "t1", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:4", track_id: "t1", confidence: 1, touched: false, original_confidence: 1 },
    ],
    parking_lot: [],
    op_id: 0,
    buckets: [
      { trackId: "t0", atomTitle: "Audio 1", atomDurationSec: 600, charsPerSec: 5 },
      { trackId: "t1", atomTitle: "Audio 2", atomDurationSec: 300, charsPerSec: 5 },
    ],
  };
  const inspection = {
    title: "Bucket Fixture",
    chapter_count: 5,
    track_count: 2,
    condition: "many_to_few" as const,
    options: ["split_proportional", "cancel"] as const,
    preselect: "split_proportional" as const,
    bucket_preview: null,
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = ${JSON.stringify(inspection)};
    window.__mappingState__.seed(${JSON.stringify(PROJECT_KEY)}, ${JSON.stringify(mapping)});
  })();`;
}

test.describe("mapping remove", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("removing a chapter drops it and renumbers", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(5);

    // Remove chapter at row index 2 (0-based)
    await page.getByTestId("chapter-remove").nth(2).click();

    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(4);
    await expect(page.getByTestId("chapter-number").last()).toHaveText("4");
    await expect(page.getByTestId("removed-strip")).toContainText("1");

    // Undo restores the chapter
    await page.getByTestId("removed-undo").click();
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(5);
  });
});
