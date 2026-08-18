import type { MappingState } from "../src/lib/ipc/bindings";
import { expect, test } from "./setup/test";
import { chapters, installMapping, pair } from "./setup/mapping-fixture";

const PROJECT_KEY = "savefail-fixture";

const mapping: MappingState = {
  pairs: [pair(0, "t0", 1), pair(1, "t0", 1), pair(2, "t1", 1)],
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

test.describe("mapping save failure notice", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters: chapters(3),
      mapping,
    });
  });

  test("a rejected op shows a transient footer notice", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(3);

    await page.evaluate(() => (window.__failNextMappingOp__ = true));
    // Chapter 3 (first row of band t1) can move up — enabled arrow.
    await page.getByTestId("chapter-move-up").nth(2).click();

    // Debounced flush is 500ms; the notice then appears.
    await expect(page.getByTestId("mapping-saved-label")).toContainText(
      "Couldn't save — change reverted",
      { timeout: 3000 },
    );
  });
});
