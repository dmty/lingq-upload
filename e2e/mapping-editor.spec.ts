import type {
  Chapter,
  MappingState,
  MismatchInspection,
} from "../src/lib/ipc/bindings";
import { expect, test } from "./setup/test";
import { installMapping, pair } from "./setup/mapping-fixture";

// Mapping editor: score-gate + rehydrate-on-reload. No DnD simulation
// (Playwright's drag harness is flaky and the pure-state contract is already
// covered in `src-tauri/tests/mapping_editor_ops.rs`). These two cases
// exercise the user-visible state machine and the persistence boundary.

const PROJECT_KEY = "mapping-fixture";

const chapters: Chapter[] = [
  { id: "idx:0", order: 0, title: "Chapter One", body: "", kind: "body" },
  { id: "idx:1", order: 1, title: "Chapter Two", body: "", kind: "body" },
];

const inspection: MismatchInspection = {
  title: "Mapping Fixture",
  chapter_count: 2,
  track_count: 2,
  condition: "count_off",
  options: ["pair_accept", "cancel"],
  preselect: "pair_accept",
  bucket_preview: null,
};

function mapping(opts: {
  withRed: boolean;
  displacedRed?: boolean;
}): MappingState {
  return {
    pairs: [
      pair(0, "t0", 0.9),
      opts.displacedRed
        ? // A displacing op bumped `confidence` to green, but the pair is
          // untouched and its original score is red — the gate must block.
          pair(1, "t1", 0.95, 0.4)
        : pair(1, "t1", opts.withRed ? 0.4 : 0.85),
    ],
    parking_lot: [],
    op_id: 0,
  };
}

test.describe("mapping editor", () => {
  test("Continue is disabled until untouched red rows are confirmed", async ({
    page,
  }) => {
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters,
      mapping: mapping({ withRed: true }),
      inspection,
    });
    await page.goto(`/match/${PROJECT_KEY}`);

    const grid = page.getByTestId("mapping-grid");
    await expect(grid).toBeVisible();

    const cont = page.getByTestId("mapping-continue");
    await expect(cont).toBeDisabled();

    // Footer never claims a save that hasn't happened.
    const savedLabel = page.getByTestId("mapping-saved-label");
    await expect(savedLabel).not.toContainText("never");
    await expect(savedLabel).not.toContainText("All changes saved");

    // Confirm the red row — idx:1. Only low-confidence rows render a Confirm,
    // so target it by chapter id. The store sends a Swap(self) to mark the
    // pair touched server-side; the gate re-evaluates from mappingState.
    await page
      .locator('[data-testid="confirm-pair"][data-chapter-id="idx:1"]')
      .click();

    await expect(cont).toBeEnabled({ timeout: 2_000 });

    // Once the debounced save lands, the footer reports it.
    await expect(savedLabel).toContainText("All changes saved", {
      timeout: 3_000,
    });
  });

  test("untouched displaced pair gates on its original confidence", async ({
    page,
  }) => {
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters,
      mapping: mapping({ withRed: false, displacedRed: true }),
      inspection,
    });
    await page.goto(`/match/${PROJECT_KEY}`);

    await expect(page.getByTestId("mapping-grid")).toBeVisible();

    // Current confidence is green (0.95) but original_confidence is red and
    // the pair is untouched — Continue must stay blocked.
    const cont = page.getByTestId("mapping-continue");
    await expect(cont).toBeDisabled();

    await page
      .locator('[data-testid="confirm-pair"][data-chapter-id="idx:1"]')
      .click();
    await expect(cont).toBeEnabled({ timeout: 2_000 });
  });

  test("state rehydrates from project.json after reload", async ({ page }) => {
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters,
      mapping: mapping({ withRed: true }),
      inspection,
    });
    await page.goto(`/match/${PROJECT_KEY}`);

    const grid = page.getByTestId("mapping-grid");
    await expect(grid).toBeVisible();

    // Touch the red row so the gate clears; this flushes through the stub's
    // cmd_apply_mapping_op so it's persisted to sessionStorage.
    await page
      .locator('[data-testid="confirm-pair"][data-chapter-id="idx:1"]')
      .click();
    await expect(page.getByTestId("mapping-continue")).toBeEnabled({
      timeout: 2_000,
    });

    // Wait for the debounced save to land in the stub's sessionStorage so
    // the rehydration assertion is meaningful (not just observing the
    // optimistic in-memory state).
    await page.waitForFunction(
      (key) => {
        const m = window.__mappingState__.byProject[key];
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
