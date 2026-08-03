import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const KEY = "course-fixture";
// The route reads its param as a joinKey identifier (same as /match and /run
// navigation elsewhere in the app). This fixture has no asin/isbn/uuid, so
// joinKey falls back to the content_hash form: `ch:<hash>`.
const ROUTE_KEY = encodeURIComponent(`ch:${KEY}`);

const seedScript = () => `
;(() => {
  window.__libraryEntries__ = [{
    id: { content_hash: "${KEY}", audible_asin: null, isbn13: null, calibre_uuid: null },
    title: "Kafka on the Shore",
    language: "ja",
    completed_lesson_count: 42,
    receipt_count: 42,
    mtime: null,
    authors: ["Haruki Murakami"],
    series: null,
    lingq_collection_id: 7,
    status: "done",
  }];
  window.__courseView__ = {
    collection: {
      id: 7, title: "Kafka on the Shore", description: null,
      level: "Intermediate 2", difficulty: 2.5, duration: 22320,
      lessons_count: 42, new_words_count: 9204, image_url: null,
      status: "private", roses_count: null, views_count: null,
    },
    lessons: [
      { id: 10, title: "The Boy Named Crow", duration: 512, word_count: 2841,
        unique_word_count: 900, new_words_count: 214, percent_completed: 100, has_audio: true },
      { id: 11, title: "Chapter Two", duration: 584, word_count: 3190,
        unique_word_count: 1010, new_words_count: 287, percent_completed: 41.5, has_audio: true },
    ],
  };
})();
`;

test.describe("course screen", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(seedScript());
  });

  test("the header renders from local data and the stat band fills from LingQ", async ({
    page,
  }) => {
    await page.goto(`/course/${ROUTE_KEY}`);

    await expect(page.getByTestId("course-header")).toContainText(
      "Kafka on the Shore",
    );
    await expect(page.getByTestId("course-header")).toContainText(
      "Haruki Murakami",
    );

    await expect(page.getByTestId("stat-lessons")).toContainText("42");
    await expect(page.getByTestId("stat-words")).toContainText("6,031");
    await expect(page.getByTestId("stat-new-words")).toContainText("9,204");
    await expect(page.getByTestId("stat-audio")).toContainText("6h 12m");
  });

  test("Open in LingQ points at the collection URL", async ({ page }) => {
    await page.goto(`/course/${ROUTE_KEY}`);

    const button = page.getByTestId("open-in-lingq");
    await expect(button).toBeVisible();
    await expect(button).toBeEnabled();

    await button.click();
    await expect
      .poll(() => page.evaluate(() => window.__openedUrl__))
      .toBe("https://www.lingq.com/ja/learn/ja/web/library/course/7");
  });

  test("the progress strip weights completion by word count", async ({ page }) => {
    await page.goto(`/course/${ROUTE_KEY}`);

    // 2841 words at 100% + 3190 words at 41.5% = 4164.85 of 6031 = 69%.
    await expect(page.getByTestId("course-progress")).toContainText("69%");
    await expect(page.getByTestId("course-progress")).toContainText("1 of 2 read");
  });

  test("each lesson gets a stat row", async ({ page }) => {
    await page.goto(`/course/${ROUTE_KEY}`);

    const rows = page.getByTestId("lesson-row");
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0)).toContainText("The Boy Named Crow");
    await expect(rows.nth(0)).toContainText("2,841");
    await expect(rows.nth(0)).toContainText("214");
    await expect(rows.nth(0)).toContainText("8:32");
    // 41.5 rounds half up to 42 (Math.round), same convention as the
    // progress-strip aggregate above.
    await expect(rows.nth(1)).toContainText("42%");
  });
});
