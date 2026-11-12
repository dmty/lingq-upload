import { expect, seed, test } from "./setup/test";
import { libraryEntry } from "./setup/library-fixture";

test("a synced row opens its LingQ course from the hover button", async ({
  page,
}) => {
  await seed(page, {
    __libraryEntries__: [
      libraryEntry("synced", {
        title: "時をかける少女",
        language: "ja",
        lingq_collection_id: 7,
      }),
      libraryEntry("local", { title: "Not uploaded", status: "idle" }),
    ],
  });
  await page.goto("/library");

  const rows = page.getByRole("option");
  const synced = rows.filter({ hasText: "時をかける少女" });
  const local = rows.filter({ hasText: "Not uploaded" });
  const button = synced.getByRole("button", { name: "Open in LingQ" });

  await expect(button).toHaveCSS("opacity", "0");
  await synced.hover();
  await expect(button).toHaveCSS("opacity", "1");
  await expect(
    local.getByRole("button", { name: "Open in LingQ" }),
  ).toHaveCount(0);

  await button.click();
  await expect
    .poll(() => page.evaluate(() => window.__openedUrl__))
    .toBe("https://www.lingq.com/ja/learn/ja/web/library/course/7");
  await expect(page).toHaveURL(/\/library$/);

  // Enter on the focused button opens LingQ, not the selected row's course.
  await page.evaluate(() => (window.__openedUrl__ = undefined));
  await page.mouse.move(0, 0);
  await page.keyboard.press("ArrowDown");
  await expect(button).toHaveCSS("opacity", "1");
  await button.focus();
  await page.keyboard.press("Enter");
  await expect
    .poll(() => page.evaluate(() => window.__openedUrl__))
    .toBe("https://www.lingq.com/ja/learn/ja/web/library/course/7");
  await expect(page).toHaveURL(/\/library$/);
});
