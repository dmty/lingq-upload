import type { MappingState, MismatchInspection } from "../src/lib/ipc/bindings";
import { expect, test } from "./setup/test";
import { chapters, installMapping, pair } from "./setup/mapping-fixture";

const PROJECT_KEY = "bucket-fixture";

const mapping: MappingState = {
  pairs: [
    pair(0, "t0", 1),
    pair(1, "t0", 1),
    pair(2, "t0", 1),
    pair(3, "t1", 1),
    pair(4, "t1", 1),
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
      atomDurationSec: 300,
      charsPerSec: 5,
      audioPath: "/x/a1.m4b",
      window: null,
    },
  ],
};

const inspection: MismatchInspection = {
  title: "Bucket Fixture",
  chapter_count: 5,
  track_count: 2,
  condition: "many_to_few",
  options: ["split_proportional", "cancel"],
  preselect: "split_proportional",
  bucket_preview: null,
};

test.describe("mapping remove", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters: chapters(5),
      mapping,
      inspection,
    });
  });

  test("removing a chapter drops it and renumbers", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(5);

    // Remove chapter at row index 2 (0-based)
    await page.getByTestId("chapter-remove").nth(2).click();

    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(4);
    await expect(page.getByTestId("chapter-number").last()).toHaveText("4");
    await expect(page.getByTestId("removed-strip")).toContainText("1");

    // Per-item restore brings the chapter back.
    await page.getByTestId("removed-restore").click();
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(5);
  });
});
