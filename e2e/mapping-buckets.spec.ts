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
    parking_lot: [], op_id: 0, partition_locked: false,
    buckets: [
      { trackId: "t0", atomTitle: "Audio 1", atomDurationSec: 600, charsPerSec: 5 },
      { trackId: "t1", atomTitle: "Audio 2", atomDurationSec: 300, charsPerSec: 5 },
    ],
  };
  // Set a non-null inspection so hydrateFromBackend doesn't redirect to /run.
  // The mapping is already seeded so the grid renders; inspection just satisfies
  // the page's "no pending decision" guard.
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

const NON_CONTIGUOUS_KEY = "nc-fixture";

function nonContiguousFixtureScript(): string {
  // 3 chapters: t0, t1, t0 — non-adjacent same track_id produces 3 distinct bands.
  const chapters = Array.from({ length: 3 }, (_, i) => ({
    id: `nc:${i}`, order: i, title: `Chapter ${i + 1}`, body: "x".repeat(100), kind: "body",
  }));
  const mapping = {
    pairs: [
      { chapter_id: "nc:0", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "nc:1", track_id: "t1", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "nc:2", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
    ],
    parking_lot: [], op_id: 0, partition_locked: false,
    buckets: [
      { trackId: "t0", atomTitle: "Audio 1", atomDurationSec: 600, charsPerSec: 5 },
      { trackId: "t1", atomTitle: "Audio 2", atomDurationSec: 300, charsPerSec: 5 },
    ],
  };
  const inspection = {
    title: "NC Fixture",
    chapter_count: 3,
    track_count: 2,
    condition: "many_to_few" as const,
    options: ["split_proportional", "cancel"] as const,
    preselect: "split_proportional" as const,
    bucket_preview: null,
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
    window.__pickerState__.chaptersByProject[${JSON.stringify(NON_CONTIGUOUS_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = ${JSON.stringify(inspection)};
    window.__mappingState__.seed(${JSON.stringify(NON_CONTIGUOUS_KEY)}, ${JSON.stringify(mapping)});
  })();`;
}

test.describe("banded bucket list", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("renders bands grouped by track with numbered chapters", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(2);
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(5);
    // chapters are numbered 1..5 in order
    await expect(page.getByTestId("chapter-number").first()).toHaveText("1");
    await expect(page.getByTestId("chapter-number").last()).toHaveText("5");
    // band header shows audio title + a formatted duration
    await expect(page.getByTestId("bucket-band-meta").first()).toContainText("Audio 1");
    await expect(page.getByTestId("bucket-band-meta").first()).toContainText("10:00");
    // no SVG connector layer
    await expect(page.locator('[data-testid="mapping-connector-layer"]')).toHaveCount(0);
  });

  test("renders 3 distinct bands for non-contiguous t0,t1,t0 track assignment", async ({ page }) => {
    await page.addInitScript(nonContiguousFixtureScript());
    await page.goto(`/match/${NON_CONTIGUOUS_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    // Non-contiguous same track_id must produce 3 separate bands (not 2).
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(3);
    // All 3 chapter rows present and numbered in order
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(3);
    await expect(page.getByTestId("chapter-number").first()).toHaveText("1");
    await expect(page.getByTestId("chapter-number").last()).toHaveText("3");
  });
});
