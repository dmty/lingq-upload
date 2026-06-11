import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

// Per-row mismatch diff inspector. Hover with 250ms debounce reveals the
// inline diff; keyboard focus and Enter/Space toggle it; Escape collapses.
// AD-025: no banner, no diagnostic copy — the highlighted spans are the
// only signal.

const PROJECT_KEY = "mapping-diff-fixture";

function fixtureScript(): string {
  const chapters = [
    { id: "idx:0", order: 0, title: "Chapter One", body: "", kind: "body" },
    { id: "idx:1", order: 1, title: "Prologue", body: "", kind: "body" },
  ];
  const inspection = {
    title: "Diff Fixture",
    chapter_count: 2,
    track_count: 2,
    condition: "count_off",
    options: ["pair_accept", "cancel"],
    preselect: "pair_accept",
    bucket_preview: null,
  };
  const mapping = {
    pairs: [
      { chapter_id: "idx:0", track_id: "Chapter_1.mp3", confidence: 0.9, touched: false },
      { chapter_id: "idx:1", track_id: "00_prologue.flac", confidence: 0.55, touched: false },
    ],
    parking_lot: [],
    op_id: 0,
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || {
      skippedByProject: {},
      chaptersByProject: {},
    };
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = ${JSON.stringify(inspection)};
    window.__mappingState__.seed(${JSON.stringify(PROJECT_KEY)}, ${JSON.stringify(mapping)});
  })();`;
}

test.describe("mapping diff inspector", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("hover with 250ms debounce reveals the inspector with diff spans", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();

    const rows = page.getByTestId("mapping-chapter-row");
    const targetRow = rows.nth(0); // "Chapter One" vs "Chapter_1.mp3"
    const inspector = page.getByTestId("mismatch-diff-inspector");

    await expect(inspector).toHaveCount(0);

    await targetRow.hover();

    // Sub-debounce check: still hidden at 100ms.
    await page.waitForTimeout(100);
    await expect(inspector).toHaveCount(0);

    // After the debounce window the inspector mounts and shows highlighted
    // segments for both add and del sides of the diff.
    await expect(inspector).toBeVisible({ timeout: 1500 });
    await expect(page.getByTestId("mismatch-diff-del").first()).toBeVisible();
    await expect(page.getByTestId("mismatch-diff-add").first()).toBeVisible();

    // Mouse-leave hides immediately.
    await page.mouse.move(0, 0);
    await expect(inspector).toHaveCount(0);
  });

  test("keyboard focus + Enter toggles, Escape collapses", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();

    const targetRow = page.getByTestId("mapping-chapter-row").nth(1);
    const inspector = page.getByTestId("mismatch-diff-inspector");

    // Focus alone reveals.
    await targetRow.focus();
    await expect(inspector).toBeVisible();

    // Enter on a focused row with no selected track toggles it closed.
    await page.keyboard.press("Enter");
    await expect(inspector).toHaveCount(0);

    // Enter again re-opens.
    await page.keyboard.press("Enter");
    await expect(inspector).toBeVisible();

    // Escape collapses.
    await page.keyboard.press("Escape");
    await expect(inspector).toHaveCount(0);
  });

  test("only one inspector visible at a time", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();

    const rows = page.getByTestId("mapping-chapter-row");

    await rows.nth(0).focus();
    await expect(page.getByTestId("mismatch-diff-inspector")).toHaveCount(1);

    await rows.nth(1).focus();
    await expect(page.getByTestId("mismatch-diff-inspector")).toHaveCount(1);
  });
});
