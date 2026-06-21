import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

// Partition lock state: a manual op (confirm-pair = swap-self) locks the
// partition and surfaces the Manual chip + Reset link. Resetting calls
// cmd_recompute_split which returns partition_locked: false.

const PROJECT_KEY = "rebalance-fixture";

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
    parking_lot: [], op_id: 0, partition_locked: false,
    buckets: [
      { trackId: "t0", atomTitle: "Audio 1", atomDurationSec: 600, charsPerSec: 5 },
      { trackId: "t1", atomTitle: "Audio 2", atomDurationSec: 300, charsPerSec: 5 },
    ],
  };
  const inspection = {
    title: "Rebalance Fixture",
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

test.describe("partition lock state", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("a manual op surfaces the Manual chip and Reset", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    // Pristine: no chip visible
    await expect(page.getByTestId("partition-manual")).toHaveCount(0);
    // Confirm a pair = swap-self op = manual edit; stub now sets partition_locked: true
    await page.getByTestId("confirm-pair").first().click();
    await expect(page.getByTestId("partition-manual")).toBeVisible({ timeout: 3000 });
    await expect(page.getByTestId("partition-reset")).toBeVisible();
    // Reset: calls cmd_recompute_split which returns partition_locked: false
    await page.getByTestId("partition-reset").click();
    await expect(page.getByTestId("partition-manual")).toHaveCount(0, { timeout: 3000 });
  });
});
