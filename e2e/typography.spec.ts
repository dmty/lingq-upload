import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

function entriesScript(): string {
  const entry = {
    id: { content_hash: "book-1", audible_asin: null, isbn13: null, calibre_uuid: null },
    title: "War and Peace",
    authors: ["Tolstoy"],
    language: "en",
    completed_lesson_count: 0,
    receipt_count: 0,
    mtime: null,
    status: "idle",
    cover_path: null,
    last_activity_at: null,
    lingq_collection_id: null,
  };
  return `window.__libraryEntries__ = ${JSON.stringify([entry])};`;
}

test.describe("typography", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(entriesScript());
  });

  test("UI is system sans (no phantom Inter), book titles are Literata", async ({ page }) => {
    await page.goto("/library");
    const bodyFont = await page.evaluate(
      () => getComputedStyle(document.body).fontFamily,
    );
    expect(bodyFont).not.toContain("Inter");

    const titleFont = await page
      .getByTestId("library-title")
      .evaluate((el) => getComputedStyle(el).fontFamily);
    expect(titleFont).toContain("Literata");
  });
});
