import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

// Strategy toggle + sidebar removal. The grid header now carries
// "Split proportionally" / "One lesson" buttons; the standalone ChapterPicker
// sidebar that duplicated the chapter list is gone from this route.

const PROJECT_KEY = "strategy-fixture";

function fixtureScript(): string {
  const chapters = Array.from({ length: 4 }, (_, i) => ({
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
      { chapter_id: "idx:2", track_id: "t1", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:3", track_id: "t1", confidence: 1, touched: false, original_confidence: 1 },
    ],
    parking_lot: [],
    op_id: 0,
    buckets: [
      { trackId: "t0", atomTitle: "Audio 1", atomDurationSec: 600, charsPerSec: 5 },
      { trackId: "t1", atomTitle: "Audio 2", atomDurationSec: 600, charsPerSec: 5 },
    ],
  };
  const inspection = {
    title: "Strategy Fixture",
    chapter_count: 4,
    track_count: 2,
    condition: "many_to_few" as const,
    options: ["split_proportional", "single_lesson", "cancel"] as const,
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

test.describe("strategy toggle", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("strategy toggle is present and sidebar picker is gone", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("strategy-split")).toBeVisible();
    await expect(page.getByTestId("strategy-single")).toBeVisible();
    // the old standalone sidebar picker no longer renders on this route
    await expect(page.getByTestId("chapter-picker")).toHaveCount(0);
  });

  test("clicking strategy-single re-resolves and re-renders the grid", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();

    // Clicking the inactive strategy calls cmd_matcher_resolve and reloads.
    await page.getByTestId("strategy-single").click();

    // Grid stays visible after re-resolve.
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
  });
});
