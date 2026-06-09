import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

// Picker persistence smoke. Loads a stub project with 6 chapters, unchecks
// two of them, navigates away then back, and asserts the same two stay
// unchecked. The persistence gate is `cmd_set_selection` round-tripping
// through the in-page state — no job is run.

const PROJECT_KEY = "picker-fixture";

function fixtureScript(): string {
  const chapters = [
    { id: "idx:0", order: 0, title: "Preface", body: "", kind: "front_matter" },
    { id: "idx:1", order: 1, title: "Chapter One", body: "", kind: "body" },
    { id: "idx:2", order: 2, title: "Chapter Two", body: "", kind: "body" },
    { id: "idx:3", order: 3, title: "Chapter Three", body: "", kind: "body" },
    { id: "idx:4", order: 4, title: "Chapter Four", body: "", kind: "body" },
    { id: "idx:5", order: 5, title: "Epilogue", body: "", kind: "back_matter" },
  ];
  // Returning a non-null mismatch inspection holds the /match route in place
  // so the picker column renders instead of bouncing to /run.
  const inspection = {
    title: "Picker Fixture",
    chapter_count: 6,
    track_count: 6,
    condition: "count_off",
    options: ["pair_accept", "cancel"],
    preselect: "pair_accept",
    bucket_preview: null,
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || {
      skippedByProject: {},
      chaptersByProject: {},
    };
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = ${JSON.stringify(inspection)};
  })();`;
}

test.describe("chapter picker", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("unchecks persist across navigation", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);

    const picker = page.getByTestId("chapter-picker");
    await expect(picker).toBeVisible();

    // Six rows render.
    const rows = picker.getByTestId("chapter-row");
    await expect(rows).toHaveCount(6);

    // Default = all checked.
    for (let i = 0; i < 6; i++) {
      await expect(rows.nth(i)).toBeChecked();
    }

    // Uncheck rows 3 and 5 (zero-indexed: idx:2 and idx:4).
    await rows.nth(2).click();
    await rows.nth(4).click();

    await expect(rows.nth(2)).not.toBeChecked();
    await expect(rows.nth(4)).not.toBeChecked();

    // Wait for debounced flush to reach the stub.
    await page.waitForFunction(
      (key) => {
        const s = (window as unknown as { __pickerState__: { skippedByProject: Record<string, string[]> } })
          .__pickerState__;
        const skipped = s.skippedByProject[key] || [];
        return skipped.includes("idx:2") && skipped.includes("idx:4");
      },
      PROJECT_KEY,
      { timeout: 2000 },
    );

    // Navigate away and back.
    await page.goto("/library");
    await page.goto(`/match/${PROJECT_KEY}`);

    const rows2 = page.getByTestId("chapter-picker").getByTestId("chapter-row");
    await expect(rows2).toHaveCount(6);
    await expect(rows2.nth(0)).toBeChecked();
    await expect(rows2.nth(1)).toBeChecked();
    await expect(rows2.nth(2)).not.toBeChecked();
    await expect(rows2.nth(3)).toBeChecked();
    await expect(rows2.nth(4)).not.toBeChecked();
    await expect(rows2.nth(5)).toBeChecked();
  });

  test("skip-front-matter chip unchecks front-matter rows non-destructively", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);

    const picker = page.getByTestId("chapter-picker");
    const rows = picker.getByTestId("chapter-row");
    await expect(rows).toHaveCount(6);

    // Manually uncheck a body row first.
    await rows.nth(1).click();
    await expect(rows.nth(1)).not.toBeChecked();

    // Toggle chip: idx:0 (front_matter) goes off; idx:1 stays off.
    await picker.getByTestId("skip-front-chip").click();
    await expect(rows.nth(0)).not.toBeChecked();
    await expect(rows.nth(1)).not.toBeChecked();

    // Toggle chip off: idx:0 restores to checked; idx:1 stays off.
    await picker.getByTestId("skip-front-chip").click();
    await expect(rows.nth(0)).toBeChecked();
    await expect(rows.nth(1)).not.toBeChecked();
  });
});
