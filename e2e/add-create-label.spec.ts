import { expect, seed, test } from "./setup/test";

test.describe("add page create-button reasons", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, {
      __languages__: [{ code: "en", title: "English", known_words: 500 }],
    });
  });

  test("the disabled Create button keeps its name and states the gap", async ({
    page,
  }) => {
    await page.goto("/add");
    // Language auto-selects the saved/first entry, so the only remaining gap
    // is content. Generous first assertion: this is the suite's first
    // navigation, so it absorbs the dev server's route compile.
    await expect(page.getByRole("button", { name: "Create" })).toBeDisabled({
      timeout: 30_000,
    });
    await expect(
      page.getByText("Add a book file or audio to continue."),
    ).toBeVisible();

    await page.evaluate(() => (window.__dialogPickPath__ = "/tmp/book.epub"));
    await page.getByRole("button", { name: "Choose book file" }).click();
    await expect(page.getByRole("button", { name: "Create" })).toBeEnabled();
    await expect(
      page.getByText("Add a book file or audio to continue."),
    ).toBeHidden();
  });

  test("audio alone is enough to create a project", async ({ page }) => {
    await page.goto("/add");
    await expect(page.getByRole("button", { name: "Create" })).toBeDisabled({
      timeout: 30_000,
    });

    await page.evaluate(() => (window.__dialogPickPath__ = "/tmp/book.m4b"));
    await page.getByRole("button", { name: "Choose audio" }).click();
    await expect(page.getByRole("button", { name: "Create" })).toBeEnabled();
  });
});
