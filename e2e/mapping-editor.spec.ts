import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

// Mapping editor: score-gate + rehydrate-on-reload. No DnD simulation
// (Playwright's drag harness is flaky and the pure-state contract is already
// covered in `src-tauri/tests/mapping_editor_ops.rs`). These two cases
// exercise the user-visible state machine and the persistence boundary.

const PROJECT_KEY = "mapping-fixture";

function fixtureScript(opts: {
  withRed: boolean;
  displacedRed?: boolean;
}): string {
  const chapters = [
    { id: "idx:0", order: 0, title: "Chapter One", body: "", kind: "body" },
    { id: "idx:1", order: 1, title: "Chapter Two", body: "", kind: "body" },
  ];
  const inspection = {
    title: "Mapping Fixture",
    chapter_count: 2,
    track_count: 2,
    condition: "count_off",
    options: ["pair_accept", "cancel"],
    preselect: "pair_accept",
    bucket_preview: null,
  };
  const mapping = {
    pairs: [
      { chapter_id: "idx:0", track_id: "t0", confidence: 0.9, touched: false },
      opts.displacedRed
        ? {
            // A displacing op bumped `confidence` to green, but the pair is
            // untouched and its original score is red — the gate must block.
            chapter_id: "idx:1",
            track_id: "t1",
            confidence: 0.95,
            original_confidence: 0.4,
            touched: false,
          }
        : {
            chapter_id: "idx:1",
            track_id: "t1",
            confidence: opts.withRed ? 0.4 : 0.85,
            touched: false,
          },
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

test.describe("mapping editor", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("Continue is disabled until untouched red rows are confirmed", async ({
    page,
  }) => {
    await page.addInitScript(fixtureScript({ withRed: true }));
    await page.goto(`/match/${PROJECT_KEY}`);

    const grid = page.getByTestId("mapping-grid");
    await expect(grid).toBeVisible();

    const cont = page.getByTestId("mapping-continue");
    await expect(cont).toBeDisabled();

    // Footer never claims a save that hasn't happened.
    const savedLabel = page.getByTestId("mapping-saved-label");
    await expect(savedLabel).not.toContainText("never");
    await expect(savedLabel).not.toContainText("All changes saved");

    // Confirm the red row — idx:1. The store sends a Swap(self) to mark the
    // pair touched server-side; the gate re-evaluates from mappingState.
    const confirmBtns = page.getByTestId("confirm-pair");
    await confirmBtns.nth(1).click();

    await expect(cont).toBeEnabled({ timeout: 2_000 });

    // Once the debounced save lands, the footer reports it.
    await expect(savedLabel).toContainText("All changes saved", {
      timeout: 3_000,
    });
  });

  test("untouched displaced pair gates on its original confidence", async ({
    page,
  }) => {
    await page.addInitScript(
      fixtureScript({ withRed: false, displacedRed: true }),
    );
    await page.goto(`/match/${PROJECT_KEY}`);

    await expect(page.getByTestId("mapping-grid")).toBeVisible();

    // Current confidence is green (0.95) but original_confidence is red and
    // the pair is untouched — Continue must stay blocked.
    const cont = page.getByTestId("mapping-continue");
    await expect(cont).toBeDisabled();

    await page.getByTestId("confirm-pair").nth(1).click();
    await expect(cont).toBeEnabled({ timeout: 2_000 });
  });

  test("state rehydrates from project.json after reload", async ({ page }) => {
    await page.addInitScript(fixtureScript({ withRed: true }));
    await page.goto(`/match/${PROJECT_KEY}`);

    const grid = page.getByTestId("mapping-grid");
    await expect(grid).toBeVisible();

    // Touch the red row so the gate clears; this flushes through the stub's
    // cmd_apply_mapping_op so it's persisted to sessionStorage.
    await page.getByTestId("confirm-pair").nth(1).click();
    await expect(page.getByTestId("mapping-continue")).toBeEnabled({
      timeout: 2_000,
    });

    // Wait for the debounced save to land in the stub's sessionStorage so
    // the rehydration assertion is meaningful (not just observing the
    // optimistic in-memory state).
    await page.waitForFunction(
      (key) => {
        const s = (
          window as unknown as {
            __mappingState__: { byProject: Record<string, { pairs: { touched?: boolean }[] }> };
          }
        ).__mappingState__;
        const m = s?.byProject?.[key];
        return !!m && m.pairs.some((p) => p.touched === true);
      },
      PROJECT_KEY,
      { timeout: 3_000 },
    );

    // Reload — the stub serves the persisted mapping back through
    // cmd_project_load, so the gate should still be open.
    await page.reload();
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("mapping-continue")).toBeEnabled({
      timeout: 2_000,
    });
  });
});
