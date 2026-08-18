import { expect, test } from "./setup/test";

const KEY = "course-nav";
const ROUTE_KEY = encodeURIComponent(`ch:${KEY}`);

const seedScript = () => `
;(() => {
  window.__libraryEntries__ = [{
    id: { content_hash: "${KEY}", audible_asin: null, isbn13: null, calibre_uuid: null },
    title: "Finished Book",
    language: "ja",
    completed_lesson_count: 2,
    receipt_count: 2,
    mtime: null,
    authors: [],
    series: null,
    lingq_collection_id: 7,
    status: "done",
  }];
  window.__courseView__ = {
    collection: {
      id: 7, title: "Finished Book", description: null, level: null,
      duration: 600, lessons_count: 2, new_words_count: 10,
      image_url: null, status: "private", roses_count: null, views_count: null,
    },
    lessons: [],
  };
})();
`;

test.describe("course navigation", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(seedScript());
  });

  test("Open on a finished library row lands on the course screen", async ({
    page,
  }) => {
    await page.goto("/library");

    await page.getByRole("button", { name: "Open" }).first().click();

    await expect(page).toHaveURL(new RegExp(`/course/${ROUTE_KEY}`));
    await expect(page.getByTestId("course-header")).toContainText(
      "Finished Book",
    );
  });
});
