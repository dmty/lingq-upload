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

test.describe("library navigation history", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, mixedLibrary);
  });

  test("a language chosen off Library opens it filtered, without inheriting a search", async ({
    page,
  }) => {
    await page.goto("/settings");
    await page.getByRole("link", { name: "Japanese", exact: true }).click();
    await expect(page).toHaveURL("/library?language=ja");
    await expect(page.locator('li[role="option"]')).toHaveCount(1);

    // A query typed in Library belongs to Library, not to the sidebar.
    await page.locator('input[type="search"]').fill("Book");
    await expect(page).toHaveURL("/library?language=ja&q=Book");
    await page.getByRole("link", { name: "Settings", exact: true }).click();
    await expect(page).toHaveURL("/settings");
    await page.getByRole("link", { name: "German", exact: true }).click();
    await expect(page).toHaveURL("/library?language=de");
  });

  test("a language chosen inside Library keeps the current search", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.locator('input[type="search"]').fill("Book");
    await page.getByRole("link", { name: "Japanese", exact: true }).click();
    await expect(page).toHaveURL("/library?language=ja&q=Book");
  });

  test("Back and Forward restore route, language and search", async ({
    page,
  }) => {
    await page.goto("/library");
    // Three separate edits: search replaces its history entry, so one Back
    // returns the finished query rather than an intermediate one.
    const input = page.locator('input[type="search"]');
    await input.fill("B");
    await input.fill("Bo");
    await input.fill("Book");
    await page.getByRole("link", { name: "Japanese", exact: true }).click();
    await expect(page).toHaveURL("/library?language=ja&q=Book");

    await page.getByRole("link", { name: "Settings", exact: true }).click();
    await expect(page).toHaveURL("/settings");

    await page.goBack();
    await expect(page).toHaveURL("/library?language=ja&q=Book");
    await expect(input).toHaveValue("Book");

    await page.goBack();
    await expect(page).toHaveURL("/library?q=Book");
    await expect(page.locator('li[role="option"]')).toHaveCount(4);

    await page.goForward();
    await expect(page).toHaveURL("/library?language=ja&q=Book");
    await expect(
      page.getByRole("link", { name: "Japanese", exact: true }),
    ).toHaveAttribute("aria-current", "page");
  });

  test("Back from a course restores the filtered and searched Library", async ({
    page,
  }) => {
    await page.goto("/library?language=ja&q=Book");
    await page.getByRole("button", { name: "Open", exact: true }).click();
    await expect(page).toHaveURL(/\/course\//);

    await page.goBack();
    await expect(page).toHaveURL("/library?language=ja&q=Book");
    await expect(page.locator('input[type="search"]')).toHaveValue("Book");
    await expect(page.locator('li[role="option"]')).toHaveCount(1);
    await expect(
      page.getByRole("link", { name: "Japanese", exact: true }),
    ).toHaveAttribute("aria-current", "page");
  });
});

// The store is only reachable from the page as a module; importing it through
// a Function keeps the seam in the test rather than adding a window bridge to
// production code.
const LOAD_STORE = `new Function("return import('/src/lib/stores/library.svelte.ts')")()`;

test.describe("languages that stop being represented", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, mixedLibrary);
  });

  test("an unknown language falls back to All and keeps the search", async ({
    page,
  }) => {
    await page.goto("/library?language=xx&q=Book");
    await expect(page).toHaveURL("/library?q=Book");
    await expect(
      page.getByRole("link", { name: "All", exact: true }),
    ).toHaveAttribute("aria-current", "page");
    await expect(page.locator('li[role="option"]')).toHaveCount(4);
  });

  test("recovery replaces the history entry instead of adding one", async ({
    page,
  }) => {
    await page.goto("/settings");
    await page.goto("/library?language=xx");
    await expect(page).toHaveURL("/library");
    await page.goBack();
    await expect(page).toHaveURL("/settings");
  });

  test("deleting the last entry of the selected language falls back to All", async ({
    page,
  }) => {
    await page.goto("/library?language=fr");
    await expect(page.locator('li[role="option"]')).toHaveCount(1);
    await page.evaluate(async (loadStore) => {
      const { library } = await eval(loadStore);
      library.removeById({
        content_hash: "book-3",
        audible_asin: null,
        isbn13: null,
        calibre_uuid: null,
      });
    }, LOAD_STORE);
    await expect(page).toHaveURL("/library");
    await expect(
      page.getByRole("link", { name: "French", exact: true }),
    ).toHaveCount(0);
  });

  test("a refresh that drops the language falls back to All", async ({
    page,
  }) => {
    await page.goto("/library?language=ja");
    await expect(page.locator('li[role="option"]')).toHaveCount(1);
    await page.evaluate(async (loadStore) => {
      window.__libraryEntries__ = (window.__libraryEntries__ ?? []).filter(
        (entry) => entry.language !== "ja",
      );
      const { library } = await eval(loadStore);
      await library.load();
    }, LOAD_STORE);
    await expect(page).toHaveURL("/library");
    await expect(
      page.getByRole("link", { name: "Japanese", exact: true }),
    ).toHaveCount(0);
    await expect(page.locator('li[role="option"]')).toHaveCount(3);
  });
});

test.describe("languages while the library is loading or unreadable", () => {
  test("an unvalidated deep link survives the first load", async ({ page }) => {
    await seed(page, mixedLibrary);
    await page.addInitScript(() => {
      window.__libraryGate__ = new Promise((resolve) => {
        window.__releaseLibrary__ = resolve;
      });
    });

    await page.goto("/library?language=ja");
    await expect(
      page.getByRole("link", { name: "All", exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Japanese", exact: true }),
    ).toHaveCount(0);
    // Nothing is known to be current yet, so nothing claims to be.
    await expect(page.locator('.source-row[aria-current="page"]')).toHaveCount(
      0,
    );
    await expect(page).toHaveURL("/library?language=ja");

    await page.evaluate(() => window.__releaseLibrary__?.());
    await expect(
      page.getByRole("link", { name: "Japanese", exact: true }),
    ).toHaveAttribute("aria-current", "page");
  });

  test("a first-load error leaves the deep link intact and offers only All", async ({
    page,
  }) => {
    await seed(page, {
      ...mixedLibrary,
      __libraryError__: { kind: "Io", message: "disk unreadable" },
    });

    await page.goto("/library?language=ja");
    await expect(page.getByText("Library is unreadable")).toBeVisible();
    await expect(page.locator(".source-destinations .source-label")).toHaveText(
      ["All"],
    );
    await expect(page.locator('.source-row[aria-current="page"]')).toHaveCount(
      0,
    );
    await expect(page).toHaveURL("/library?language=ja");
  });

  test("a later error keeps the languages the last good load produced", async ({
    page,
  }) => {
    await seed(page, mixedLibrary);
    await page.goto("/library?language=ja");
    await expect(page.locator('li[role="option"]')).toHaveCount(1);

    await page.evaluate(async (loadStore) => {
      window.__libraryError__ = { kind: "Io", message: "disk unreadable" };
      const { library } = await eval(loadStore);
      await library.load();
    }, LOAD_STORE);

    await expect(page.getByText("Library is unreadable")).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Japanese", exact: true }),
    ).toHaveAttribute("aria-current", "page");
    await expect(page).toHaveURL("/library?language=ja");
  });
});
