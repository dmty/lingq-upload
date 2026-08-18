import { expect, test } from "./setup/test";

const seed = `;(() => {
  window.__languages__ = [{ code: "en", title: "English", known_words: 500 }];
})();`;

test.describe("add page create-button reasons", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(seed);
  });

  test("the disabled Create button says what's missing", async ({ page }) => {
    await page.goto("/add");
    // Language auto-selects the saved/first entry; text file is the first gap.
    // Generous first assertion: this is the suite's first navigation, so it
    // absorbs the dev server's route compile.
    await expect(
      page.getByRole("button", { name: "Add the book file" }),
    ).toBeDisabled({ timeout: 30_000 });

    await page.evaluate(() => (window.__dialogPickPath__ = "/tmp/book.epub"));
    await page
      .getByRole("button", { name: "Drop chapter text or click to choose" })
      .click();
    await expect(
      page.getByRole("button", { name: "Add the audio" }),
    ).toBeDisabled();

    await page.evaluate(() => (window.__dialogPickPath__ = "/tmp/book.m4b"));
    await page
      .getByRole("button", { name: "Drop audio or click to choose" })
      .click();
    await expect(page.getByRole("button", { name: "Create" })).toBeEnabled();
  });
});
