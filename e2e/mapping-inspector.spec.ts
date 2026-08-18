import type { MappingState, MismatchInspection } from "../src/lib/ipc/bindings";
import { expect, test } from "./setup/test";
import { installMapping, pair } from "./setup/mapping-fixture";

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
      audioPath: "/audio/t0.m4a",
      window: [0, 600],
    },
    {
      trackId: "t1",
      atomTitle: "Audio 2",
      atomDurationSec: 300,
      charsPerSec: 5,
      audioPath: "/audio/t1.m4a",
      window: [0, 300],
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

test.describe("chapter inspector", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, { key: PROJECT_KEY, mapping, inspection });
  });

  test("selecting a row shows the chapter text in the inspector", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("chapter-inspector")).toHaveCount(0); // nothing selected yet

    await page.getByTestId("mapping-chapter-row").nth(0).click();
    await expect(page.getByTestId("chapter-inspector")).toBeVisible();
    await expect(page.getByTestId("inspector-text")).toContainText("x"); // body is "x".repeat(100)
  });

  test("removing the selected chapter advances inspector to the next chapter", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await page.getByTestId("mapping-chapter-row").nth(1).click();
    await expect(page.getByTestId("chapter-inspector")).toContainText(
      "Chapter 2",
    );
    await page.getByTestId("inspector-remove").click();
    await expect(page.getByTestId("chapter-inspector")).toContainText(
      "Chapter 3",
    );
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(4);
  });

  test("removing the last chapter falls back to the previous one", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await page.getByTestId("mapping-chapter-row").nth(4).click();
    await expect(page.getByTestId("chapter-inspector")).toContainText(
      "Chapter 5",
    );
    await page.getByTestId("inspector-remove").click();
    await expect(page.getByTestId("chapter-inspector")).toContainText(
      "Chapter 4",
    );
  });

  test("inspector renders a windowed audio element for the bucket", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await page.getByTestId("mapping-chapter-row").nth(0).click();
    // The native <audio> is driven by a custom transport, so it is hidden;
    // assert it carries the window bounds and that the play control shows.
    const audio = page.getByTestId("inspector-audio");
    await expect(audio).toHaveAttribute("data-window-start", /\d/);
    await expect(audio).toHaveAttribute("data-window-end", /\d/);
    await expect(page.getByTestId("inspector-play")).toBeVisible();
  });

  test("the ↓ arrow on a bucket's last row moves it to the next bucket", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    // band t0={0,1,2}, t1={3,4}. Arrows render on every row; only t0's last
    // row (idx:2) has an enabled ↓ arrow.
    const downArrow = page.locator(
      '[data-testid="chapter-move-down"][data-chapter-id="idx:2"]',
    );
    await expect(downArrow).toBeEnabled();
    await downArrow.click();
    // boundary shifts: t0 now has 2 rows, t1 has 3 — still 2 bands.
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(2);
    const secondBand = page.getByTestId("mapping-bucket-band").nth(1);
    await expect(secondBand.getByTestId("mapping-chapter-row")).toHaveCount(3);
  });

  test("the ↑ arrow on a bucket's first row moves it to the previous bucket", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    // Only t1's first row (idx:3) has an enabled ↑ arrow.
    const upArrow = page.locator(
      '[data-testid="chapter-move-up"][data-chapter-id="idx:3"]',
    );
    await expect(upArrow).toBeEnabled();
    await upArrow.click();
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(2);
    const firstBand = page.getByTestId("mapping-bucket-band").nth(0);
    await expect(firstBand.getByTestId("mapping-chapter-row")).toHaveCount(4);
  });

  test("move arrows render on every row, enabled only at band edges", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    // 5 rows total, so 5 ↓ arrows and 5 ↑ arrows render — disabled unless the
    // row sits on a band edge (t0 last = idx:2 for ↓, t1 first = idx:3 for ↑).
    await expect(page.getByTestId("chapter-move-down")).toHaveCount(5);
    await expect(page.getByTestId("chapter-move-up")).toHaveCount(5);

    const enabledDown = new Set(["idx:2"]);
    const enabledUp = new Set(["idx:3"]);
    for (const id of ["idx:0", "idx:1", "idx:2", "idx:3", "idx:4"]) {
      const downArrow = page.locator(
        `[data-testid="chapter-move-down"][data-chapter-id="${id}"]`,
      );
      const upArrow = page.locator(
        `[data-testid="chapter-move-up"][data-chapter-id="${id}"]`,
      );
      if (enabledDown.has(id)) {
        await expect(downArrow).toBeEnabled();
      } else {
        await expect(downArrow).toBeDisabled();
      }
      if (enabledUp.has(id)) {
        await expect(upArrow).toBeEnabled();
      } else {
        await expect(upArrow).toBeDisabled();
      }
    }
  });

  test("remove from the inspector drops the chapter", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await page.getByTestId("mapping-chapter-row").nth(0).click();
    await page.getByTestId("inspector-remove").click();
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(4);
    await expect(page.getByTestId("removed-strip")).toContainText("1");
  });
});
