import type { MappingState, MismatchInspection } from "../src/lib/ipc/bindings";
import { expect, test } from "./setup/test";
import { chapters, installMapping, pair } from "./setup/mapping-fixture";

// Strategy toggle + sidebar removal. The grid header now carries
// "Split proportionally" / "One lesson" buttons; the standalone ChapterPicker
// sidebar that duplicated the chapter list is gone from this route.

const PROJECT_KEY = "strategy-fixture";

const mapping: MappingState = {
  pairs: [
    pair(0, "t0", 1),
    pair(1, "t0", 1),
    pair(2, "t1", 1),
    pair(3, "t1", 1),
  ],
  parking_lot: [],
  op_id: 0,
  buckets: [
    {
      trackId: "t0",
      atomTitle: "Audio 1",
      atomDurationSec: 600,
      charsPerSec: 5,
      audioPath: "/x/a0.m4b",
      window: null,
    },
    {
      trackId: "t1",
      atomTitle: "Audio 2",
      atomDurationSec: 600,
      charsPerSec: 5,
      audioPath: "/x/a1.m4b",
      window: null,
    },
  ],
};

const inspection: MismatchInspection = {
  title: "Strategy Fixture",
  chapter_count: 4,
  track_count: 2,
  condition: "many_to_few",
  options: ["split_proportional", "single_lesson", "cancel"],
  preselect: "split_proportional",
  bucket_preview: null,
};

test.describe("strategy toggle", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters: chapters(4),
      mapping,
      inspection,
    });
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
