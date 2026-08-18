import type { Chapter, MappingState } from "../src/lib/ipc/bindings";
import { expect, test } from "./setup/test";
import { installMapping, pair } from "./setup/mapping-fixture";

const PROJECT_KEY = "inspector-empty-fixture";

const chapters: Chapter[] = Array.from({ length: 2 }, (_, i) => ({
  id: `idx:${i}`,
  order: i,
  title: `Chapter ${i + 1}`,
  body: "Lorem ipsum ".repeat(20),
  kind: "body",
}));

const mapping: MappingState = {
  pairs: [pair(0, "t0", 1), pair(1, "t0", 1)],
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
  ],
};

test.describe("inspector empty state", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, { key: PROJECT_KEY, chapters, mapping });
  });

  test("prompt before selection, inspector after", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("chapter-inspector-empty")).toContainText(
      /select a chapter/i,
    );

    await page.getByTestId("mapping-chapter-row").first().click();
    await expect(page.getByTestId("chapter-inspector")).toBeVisible();
    await expect(page.getByTestId("chapter-inspector-empty")).toHaveCount(0);
  });
});
