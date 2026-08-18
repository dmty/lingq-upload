import type { MappingState } from "../src/lib/ipc/bindings";
import { expect, test } from "./setup/test";
import { installMapping, pair } from "./setup/mapping-fixture";

const PROJECT_KEY = "gate-fixture";

const mapping: MappingState = {
  pairs: [
    pair(0, "t0", 1),
    pair(1, "t0", 1),
    pair(2, "t0", 1),
    pair(3, "t1", 0.3),
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

test.describe("mapping grid gate + arrows", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, { key: PROJECT_KEY, mapping });
  });

  test("gate reason is inline and clears on confirm", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-continue")).toBeDisabled();
    await expect(page.getByTestId("continue-blockers")).toContainText(
      "1 low-confidence pair needs review",
    );

    await page.getByTestId("confirm-pair").click();
    await expect(page.getByTestId("mapping-continue")).toBeEnabled();
    await expect(page.getByTestId("continue-blockers")).toHaveCount(0);
  });

  test("every row renders both arrows; only legal ones are enabled", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("chapter-move-up")).toHaveCount(5);
    await expect(page.getByTestId("chapter-move-down")).toHaveCount(5);

    // Mid-band row (Chapter 2) — both disabled.
    await expect(page.getByTestId("chapter-move-up").nth(1)).toBeDisabled();
    await expect(page.getByTestId("chapter-move-down").nth(1)).toBeDisabled();
    // First row of band 2 (Chapter 4) can move up.
    await expect(page.getByTestId("chapter-move-up").nth(3)).toBeEnabled();
  });
});
