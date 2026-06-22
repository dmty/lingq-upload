import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const PROJECT_KEY = "bucket-fixture";

function fixtureScript(): string {
  const chapters = Array.from({ length: 5 }, (_, i) => ({
    id: `idx:${i}`, order: i, title: `Chapter ${i + 1}`, body: "x".repeat(100), kind: "body",
  }));
  const mapping = {
    pairs: [
      { chapter_id: "idx:0", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:1", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:2", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:3", track_id: "t1", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:4", track_id: "t1", confidence: 1, touched: false, original_confidence: 1 },
    ],
    parking_lot: [], op_id: 0,
    buckets: [
      { trackId: "t0", atomTitle: "Audio 1", atomDurationSec: 600, charsPerSec: 5, audioPath: "/audio/t0.m4a", window: [0, 600] },
      { trackId: "t1", atomTitle: "Audio 2", atomDurationSec: 300, charsPerSec: 5, audioPath: "/audio/t1.m4a", window: [0, 300] },
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

test.describe("chapter inspector", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("selecting a row shows the chapter text in the inspector", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("chapter-inspector")).toHaveCount(0); // nothing selected yet

    await page.getByTestId("mapping-chapter-row").nth(0).click();
    await expect(page.getByTestId("chapter-inspector")).toBeVisible();
    await expect(page.getByTestId("inspector-text")).toContainText("x"); // body is "x".repeat(100)
  });

  test("inspector renders a windowed audio element for the bucket", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await page.getByTestId("mapping-chapter-row").nth(0).click();
    const audio = page.getByTestId("inspector-audio");
    await expect(audio).toBeVisible();
    // window for Audio 1 in the fixture is the whole atom (0..600) -> data attrs present
    await expect(audio).toHaveAttribute("data-window-start", /\d/);
    await expect(audio).toHaveAttribute("data-window-end", /\d/);
  });

  test("move reassigns an edge chapter to the adjacent bucket", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    // select the last chapter of bucket t0 (row index 2)
    await page.getByTestId("mapping-chapter-row").nth(2).click();
    await expect(page.getByTestId("chapter-inspector")).toBeVisible();
    // move it to the adjacent audio (t1)
    await page.getByTestId("inspector-move").click();
    await page.getByTestId("inspector-move-option").first().click();
    // band t0 now has 2 rows, band t1 has 3 — bands still 2, but boundary moved
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(2);
    // the moved row is now under the second band
    const secondBand = page.getByTestId("mapping-bucket-band").nth(1);
    await expect(secondBand.getByTestId("mapping-chapter-row")).toHaveCount(3);
  });

  test("remove from the inspector drops the chapter", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await page.getByTestId("mapping-chapter-row").nth(0).click();
    await page.getByTestId("inspector-remove").click();
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(4);
    await expect(page.getByTestId("removed-strip")).toContainText("1");
  });
});
