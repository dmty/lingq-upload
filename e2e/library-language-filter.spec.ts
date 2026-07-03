import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

function entriesScript(): string {
  const entry = (i: number, language: string, status: string) => ({
    id: { content_hash: `book-${i}`, audible_asin: null, isbn13: null, calibre_uuid: null },
    title: `Book ${i}`,
    authors: ["Author"],
    language,
    completed_lesson_count: 0,
    receipt_count: 0,
    mtime: null,
    status,
    cover_path: null,
    last_activity_at: null,
    lingq_collection_id: status === "done" ? 42 : null,
    series: null,
    failed_reason: null,
  });
  return `window.__libraryEntries__ = ${JSON.stringify([
    entry(1, "de", "done"),
    entry(2, "fr", "idle"),
  ])};`;
}

test.describe("library filter language names + badge casing", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(entriesScript());
  });

  test("filter shows display names, value stays the code", async ({ page }) => {
    await page.goto("/library");
    const select = page.locator("select");
    await expect(select.locator("option", { hasText: "German" })).toHaveCount(1);
    await select.selectOption("de");
    await expect(page.locator('[role="option"]')).toHaveCount(1);
  });

  test("status badges are sentence case", async ({ page }) => {
    await page.goto("/library");
    // Wait for entries to load (we have 2)
    await expect(page.locator('li[role="option"]')).toHaveCount(2);
    await expect(page.getByText("Done")).toBeVisible();
  });
});
