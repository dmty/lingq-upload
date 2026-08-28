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
      inspection: null,
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

  // The route heading that used to separate the indicator from the copy below
  // it is sr-only now, and sr-only is out of flow — without a gap of its own
  // the indicator collides with the next line.
  test("the indicator keeps a gap above the copy beneath it", async ({
    page,
  }) => {
    for (const route of ["/add", `/match/${PROJECT_KEY}`]) {
      await page.goto(route);
      await page.waitForLoadState("networkidle");
      const gap = await page
        .getByTestId("step-indicator")
        .evaluate((el) => {
          let next = el.nextElementSibling;
          while (next && getComputedStyle(next).position === "absolute") {
            next = next.nextElementSibling;
          }
          return (
            next!.getBoundingClientRect().top -
            el.getBoundingClientRect().bottom
          );
        });
      expect(gap).toBeGreaterThanOrEqual(8);
    }
  });
});
