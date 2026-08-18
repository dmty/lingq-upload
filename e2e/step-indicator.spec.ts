import type { MappingState } from "../src/lib/ipc/bindings";
import { expect, test } from "./setup/test";
import { chapters, installMapping, pair } from "./setup/mapping-fixture";

const PROJECT_KEY = "steps-fixture";

const mapping: MappingState = {
  pairs: [pair(0, "t0", 1)],
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

test.describe("pipeline step indicator", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters: chapters(1),
      mapping,
    });
  });

  test("add page marks step 1 current", async ({ page }) => {
    await page.goto("/add");
    const indicator = page.getByTestId("step-indicator");
    await expect(indicator).toBeVisible();
    await expect(indicator.locator('[aria-current="step"]')).toContainText(
      "Add",
    );
  });

  test("match page marks step 2 current", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    const indicator = page.getByTestId("step-indicator");
    await expect(indicator.locator('[aria-current="step"]')).toContainText(
      "Match",
    );
  });
});
