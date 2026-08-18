import { expect, test } from "./setup/test";

function entriesScript(): string {
  const entry = (i: number) => ({
    id: {
      content_hash: `book-${i}`,
      audible_asin: null,
      isbn13: null,
      calibre_uuid: null,
    },
    title: `Book ${i}`,
    authors: ["Author"],
    language: "en",
    completed_lesson_count: 0,
    receipt_count: 0,
    mtime: null,
    status: "idle",
    cover_path: null,
    last_activity_at: null,
    lingq_collection_id: null,
  });
  return `window.__libraryEntries__ = ${JSON.stringify([entry(1), entry(2), entry(3)])};`;
}

test.describe("library focus reset on filter change", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(entriesScript());
  });

  test("typing in search clears the keyboard focus index", async ({ page }) => {
    await page.goto("/library");
    await expect(page.locator('[role="option"]')).toHaveCount(3);

    // Focus row 0 (Book 1). Filtering to Book 3 collapses the list to a
    // single row that also sits at index 0 — a stale focusIndex of 0 would
    // wrongly land on it.
    await page.keyboard.press("ArrowDown");
    await expect(
      page.locator('[role="option"][aria-selected="true"]'),
    ).toHaveCount(1);

    await page.locator('input[type="search"]').fill("Book 3");
    await expect(page.locator('[role="option"]')).toHaveCount(1);
    // Focus must not silently point at the only remaining row.
    await expect(
      page.locator('[role="option"][aria-selected="true"]'),
    ).toHaveCount(0);
  });
});
