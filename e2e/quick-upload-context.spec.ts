import { expect, seed, test } from "./setup/test";
import { libraryEntry } from "./setup/library-fixture";

const KEY = "quick-upload-ctx";
const ROUTE_KEY = encodeURIComponent(`ch:${KEY}`);

test.describe("quick upload destination context", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, {
      __libraryEntries__: [
        libraryEntry(KEY, {
          title: "Context Book",
          language: "ja",
          lingq_collection_id: 7,
        }),
      ],
      __languages__: [
        { code: "en", title: "English", known_words: 500 },
        { code: "ja", title: "Japanese", known_words: 0 },
      ],
      __collections__: [
        { id: 7, title: "Course A" },
        { id: 9, title: "Course B" },
      ],
    });
  });

  test("a course carries its language and collection into quick upload", async ({
    page,
  }) => {
    await page.goto(`/course/${ROUTE_KEY}`);
    await page
      .getByTestId("app-toolbar")
      .getByRole("link", { name: "Quick upload" })
      .click();

    await expect(page).toHaveURL(/language=ja&collection=7/);
    await expect(page.locator("select").first()).toHaveValue("ja");
    await expect(page.locator("select").nth(1)).toHaveValue("7");
  });

  test("a language-filtered library carries just the language", async ({
    page,
  }) => {
    await page.goto("/library?language=ja");
    await page
      .getByTestId("app-toolbar")
      .getByRole("link", { name: "Quick upload" })
      .click();

    await expect(page).toHaveURL(/\/upload\?language=ja$/);
    await expect(page.locator("select").first()).toHaveValue("ja");
    await expect(page.locator("select").nth(1)).toHaveValue("");
  });

  test("no destination context leaves the pickers empty", async ({ page }) => {
    await page.goto("/library");
    await page
      .getByTestId("app-toolbar")
      .getByRole("link", { name: "Quick upload" })
      .click();

    await expect(page).toHaveURL(/\/upload$/);
    await expect(page.locator("select").first()).toHaveValue("");
  });
});
