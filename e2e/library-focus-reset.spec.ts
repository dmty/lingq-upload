import { expect, seed, test } from "./setup/test";
import { libraryEntry } from "./setup/library-fixture";

test.describe("library focus reset on filter change", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, {
      __libraryEntries__: [1, 2, 3].map((i) =>
        libraryEntry(`book-${i}`, {
          title: `Book ${i}`,
          authors: ["Author"],
          status: "idle",
          language: i === 3 ? "ja" : "en",
        }),
      ),
    });
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
    await expect(page).toHaveURL("/library?q=Book+3");
    await expect(page.locator('[role="option"]')).toHaveCount(1);
    // Focus must not silently point at the only remaining row.
    await expect(
      page.locator('[role="option"][aria-selected="true"]'),
    ).toHaveCount(0);
  });

  test("choosing a language clears the keyboard focus index", async ({
    page,
  }) => {
    await page.goto("/library");
    await expect(page.locator('[role="option"]')).toHaveCount(3);

    await page.keyboard.press("ArrowDown");
    await expect(
      page.locator('[role="option"][aria-selected="true"]'),
    ).toHaveCount(1);

    await page.getByRole("link", { name: "Japanese", exact: true }).click();
    await expect(page).toHaveURL("/library?language=ja");
    await expect(page.locator('[role="option"]')).toHaveCount(1);
    await expect(
      page.locator('[role="option"][aria-selected="true"]'),
    ).toHaveCount(0);
  });
});
