import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const PROJECT_KEY = "skip-fixture";

function fixtureScript(): string {
  const chapters = [
    { id: "idx:0", order: 0, title: "Preface", body: "x".repeat(50), kind: "front_matter" },
    { id: "idx:1", order: 1, title: "Chapter One", body: "x".repeat(100), kind: "body" },
    { id: "idx:2", order: 2, title: "Chapter Two", body: "x".repeat(100), kind: "body" },
    { id: "idx:3", order: 3, title: "Chapter Three", body: "x".repeat(100), kind: "body" },
    { id: "idx:4", order: 4, title: "Epilogue", body: "x".repeat(50), kind: "back_matter" },
  ];
  const mapping = {
    pairs: [
      { chapter_id: "idx:0", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:1", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:2", track_id: "t1", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:3", track_id: "t1", confidence: 1, touched: false, original_confidence: 1 },
      { chapter_id: "idx:4", track_id: "t1", confidence: 1, touched: false, original_confidence: 1 },
    ],
    parking_lot: [],
    op_id: 0,
    buckets: [
      { trackId: "t0", atomTitle: "Audio 1", atomDurationSec: 600, charsPerSec: 5 },
      { trackId: "t1", atomTitle: "Audio 2", atomDurationSec: 300, charsPerSec: 5 },
    ],
  };
  const inspection = {
    title: "Skip Fixture",
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

test.describe("bulk matter toggle", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
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
        const s = (
          window as unknown as {
            __pickerState__: { skippedByProject: Record<string, string[]> };
          }
        ).__pickerState__;
        const skipped = s.skippedByProject[key] || [];
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
