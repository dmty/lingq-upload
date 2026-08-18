import { expect, seed, test } from "./setup/test";
import { libraryEntry } from "./setup/library-fixture";
import type { LibraryStatus } from "../src/lib/ipc/bindings";

const entry = (i: number, language: string, status: LibraryStatus) =>
  libraryEntry(`book-${i}`, {
    title: `Book ${i}`,
    authors: ["Author"],
    language,
    status,
    lingq_collection_id: status === "done" ? 42 : null,
  });

test.describe("library filter language names + badge casing", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, {
      __libraryEntries__: [entry(1, "de", "done"), entry(2, "fr", "idle")],
    });
  });

  test("filter shows display names, value stays the code", async ({ page }) => {
    await page.goto("/library");
    const select = page.locator("select");
    await expect(select.locator("option", { hasText: "German" })).toHaveCount(
      1,
    );
    await select.selectOption("de");
    await expect(page.locator('[role="option"]')).toHaveCount(1);
  });

  test("status badges are sentence case", async ({ page }) => {
    await page.goto("/library");
    // Wait for entries to load (we have 2)
    await expect(page.locator('li[role="option"]')).toHaveCount(2);
    // Case-sensitive regex: plain getByText("Done") matches lowercase "done"
    // too; exact:true fails because the badge also contains the icon glyph.
    await expect(page.getByText(/Done/).first()).toBeVisible();
  });
});
