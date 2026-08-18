import { expect, test } from "./setup/test";
import { installMapping, pair } from "./setup/mapping-fixture";
import type {
  BucketMeta,
  Chapter,
  MismatchInspection,
} from "../src/lib/ipc/bindings";

const PROJECT_KEY = "skip-fixture";

const chapters: Chapter[] = [
  {
    id: "idx:0",
    order: 0,
    title: "Preface",
    body: "x".repeat(50),
    kind: "front_matter",
  },
  {
    id: "idx:1",
    order: 1,
    title: "Chapter One",
    body: "x".repeat(100),
    kind: "body",
  },
  {
    id: "idx:2",
    order: 2,
    title: "Chapter Two",
    body: "x".repeat(100),
    kind: "body",
  },
  {
    id: "idx:3",
    order: 3,
    title: "Chapter Three",
    body: "x".repeat(100),
    kind: "body",
  },
  {
    id: "idx:4",
    order: 4,
    title: "Epilogue",
    body: "x".repeat(50),
    kind: "back_matter",
  },
];

const buckets: BucketMeta[] = [
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
];

const mapping = {
  pairs: [
    pair(0, "t0", 1),
    pair(1, "t0", 1),
    pair(2, "t1", 1),
    pair(3, "t1", 1),
    pair(4, "t1", 1),
  ],
  parking_lot: [],
  op_id: 0,
  buckets,
};

const inspection: MismatchInspection = {
  title: "Skip Fixture",
  chapter_count: 5,
  track_count: 2,
  condition: "many_to_few",
  options: ["split_proportional", "cancel"],
  preselect: "split_proportional",
  bucket_preview: null,
};

test.describe("bulk matter toggle", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters,
      mapping,
      inspection,
    });
  });

  test("skip-matter-chip removes front/back matter and restores non-destructively", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(5);

    // Manually remove a body chapter first to verify non-destructive toggle.
    await page.getByTestId("chapter-remove").nth(1).click(); // remove Chapter One
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(4);

    // Click skip-matter-chip — front_matter (idx:0) and back_matter (idx:4) should be removed.
    const chip = page.getByTestId("skip-matter-chip");
    await expect(chip).toBeVisible();
    await expect(chip).toContainText("Remove front & back matter");
    await chip.click();

    // 2 matter chapters removed on top of the already-removed body chapter = 2 matter + 1 body = 3 removed.
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(2);
    await expect(page.getByTestId("removed-strip")).toBeVisible();
    await expect(chip).toContainText("Restore front & back matter");

    // Click again — only matter chapters restore; the manually-removed body stays removed.
    await chip.click();
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(4);
    await expect(chip).toContainText("Remove front & back matter");
  });

  test("selection persists across navigation", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();

    // Use the bulk chip to skip matter chapters.
    await page.getByTestId("skip-matter-chip").click();
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(3);

    // Wait for the stub to persist the skip.
    await page.waitForFunction(
      (key) => {
        const skipped = window.__pickerState__.skippedByProject[key] || [];
        return skipped.includes("idx:0") && skipped.includes("idx:4");
      },
      PROJECT_KEY,
      { timeout: 2000 },
    );

    // Navigate away and back.
    await page.goto("/library");
    await page.goto(`/match/${PROJECT_KEY}`);

    // Matter chapters should still be absent from mapping rows.
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(3);
    await expect(page.getByTestId("removed-strip")).toBeVisible();
    await expect(page.getByTestId("skip-matter-chip")).toContainText(
      "Restore front & back matter",
    );
  });
});
