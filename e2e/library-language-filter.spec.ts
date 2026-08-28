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

test.describe("library badge casing", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, {
      __libraryEntries__: [entry(1, "de", "done"), entry(2, "fr", "idle")],
    });
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

const mixedLibrary = {
  __libraryEntries__: [
    entry(1, "ja", "done"),
    entry(2, "de", "idle"),
    entry(3, "fr", "idle"),
    entry(4, "de", "done"),
  ],
};

test.describe("sidebar language destinations", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, mixedLibrary);
  });

  test("languages appear once, in display-name order, without icons", async ({
    page,
  }) => {
    // Opened away from Library: the shell owns the load, so destinations are
    // populated wherever the app starts.
    await page.goto("/settings");
    const nav = page.getByRole("navigation", { name: "Sections" });
    await expect(
      nav.getByRole("link", { name: "All", exact: true }),
    ).toBeVisible();
    await expect(page.locator(".source-destinations .source-label")).toHaveText([
      "All",
      "French",
      "German",
      "Japanese",
    ]);
    await expect(page.locator(".source-destinations svg")).toHaveCount(0);
    await expect(
      nav.getByRole("link", { name: "German", exact: true }),
    ).toHaveAttribute("href", "/library?language=de");
  });
});

test.describe("URL-owned library filters", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, mixedLibrary);
  });

  test("a language URL filters the list and marks its destination current", async ({
    page,
  }) => {
    await page.goto("/library?language=de");
    await expect(page.locator('li[role="option"]')).toHaveCount(2);
    await expect(
      page.getByRole("link", { name: "German", exact: true }),
    ).toHaveAttribute("aria-current", "page");
    await expect(page.locator("select")).toHaveCount(0);
    await expect(page.locator('input[type="search"]')).toHaveValue("");

    await page.locator('input[type="search"]').fill("Book 4");
    await expect(page).toHaveURL("/library?language=de&q=Book+4");
    await expect(page.locator('li[role="option"]')).toHaveCount(1);
  });

  // `appearance: none` strips WebKit's built-in ⊗, so the field brings its
  // own — and it drops only the query, never the sidebar's language.
  test("clearing the search keeps the language and refocuses the field", async ({
    page,
  }) => {
    await page.goto("/library?language=de&q=Book+4");
    const input = page.locator('input[type="search"]');
    await expect(input).toHaveValue("Book 4");
    await expect(page.locator('li[role="option"]')).toHaveCount(1);

    await page.getByRole("button", { name: "Clear search" }).click();
    await expect(page).toHaveURL("/library?language=de");
    await expect(page.locator('li[role="option"]')).toHaveCount(2);
    await expect(input).toBeFocused();
  });
});
