import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const KEY = "course-cache";
// Same joinKey fallback as course-stats.spec.ts: no asin/isbn/uuid on the
// fixture, so the route resolves the `ch:<hash>` form.
const ROUTE_KEY = encodeURIComponent(`ch:${KEY}`);

const seedScript = () => `
;(() => {
  window.__libraryEntries__ = [{
    id: { content_hash: "${KEY}", audible_asin: null, isbn13: null, calibre_uuid: null },
    title: "Cached Course",
    language: "ja",
    completed_lesson_count: 1,
    receipt_count: 1,
    mtime: null,
    authors: [],
    series: null,
    lingq_collection_id: 7,
    status: "done",
  }];
  window.__courseView__ = {
    collection: {
      id: 7, title: "Cached Course", description: null, level: null,
      difficulty: null, duration: 600, lessons_count: 1, new_words_count: 10,
      image_url: null, status: "private", roses_count: null, views_count: null,
    },
    lessons: [
      { id: 10, title: "Only Chapter", duration: 600, word_count: 100,
        unique_word_count: 80, new_words_count: 10, percent_completed: 0, has_audio: true },
    ],
  };
})();
`;

const fetchCount = (page: import("@playwright/test").Page, workerIndex: number) =>
  page.evaluate(
    (ns) => Number(sessionStorage.getItem("__courseFetchCount__:" + ns) || "0"),
    String(workerIndex),
  );

// Leave and return through client-side navigation. A `page.goto` back to the
// course route would be a full document load, which reloads the store module
// and empties the cache — the suppression under test would never be exercised.
async function leaveAndReturn(page: import("@playwright/test").Page) {
  await page.getByRole("link", { name: "Back to Library" }).click();
  await expect(page).toHaveURL(/\/library/);
  await page.goBack();
}

test.describe("course stats caching", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.clock.install();
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(seedScript());
  });

  test("a revisit inside the TTL does not refetch", async ({ page }, testInfo) => {
    await page.goto(`/course/${ROUTE_KEY}`);
    await expect(page.getByTestId("stat-lessons")).toContainText("1");
    expect(await fetchCount(page, testInfo.workerIndex)).toBe(1);

    // Proves the label ticks on a live, still-mounted screen — not just that
    // it's computed fresh on remount.
    await expect(page.getByTestId("course-freshness")).toContainText("just now");
    await page.clock.fastForward("05:00");
    await expect(page.getByTestId("course-freshness")).toContainText("5 minutes ago");

    await leaveAndReturn(page);
    await expect(page.getByTestId("stat-lessons")).toContainText("1");

    expect(await fetchCount(page, testInfo.workerIndex)).toBe(1);
    await expect(page.getByTestId("course-freshness")).toContainText("5 minutes ago");
  });

  test("a revisit past the TTL revalidates in the background", async ({
    page,
  }, testInfo) => {
    await page.goto(`/course/${ROUTE_KEY}`);
    await expect(page.getByTestId("stat-lessons")).toContainText("1");
    expect(await fetchCount(page, testInfo.workerIndex)).toBe(1);

    await page.clock.fastForward("16:00");
    await leaveAndReturn(page);

    await expect(page.getByTestId("stat-lessons")).toContainText("1");
    await expect
      .poll(async () => await fetchCount(page, testInfo.workerIndex))
      .toBe(2);
  });

  test("Refresh forces a fetch on a fresh entry", async ({ page }, testInfo) => {
    await page.goto(`/course/${ROUTE_KEY}`);
    await expect(page.getByTestId("stat-lessons")).toContainText("1");
    expect(await fetchCount(page, testInfo.workerIndex)).toBe(1);

    await page.getByTestId("course-refresh").click();

    await expect.poll(async () => await fetchCount(page, testInfo.workerIndex)).toBe(2);
  });

  test("Refresh shows the revalidating indicator while the fetch is held", async ({
    page,
  }, testInfo) => {
    await page.goto(`/course/${ROUTE_KEY}`);
    await expect(page.getByTestId("stat-lessons")).toContainText("1");
    expect(await fetchCount(page, testInfo.workerIndex)).toBe(1);

    await page.evaluate(() => {
      window.__courseGate__ = new Promise((resolve) => {
        window.__releaseCourse__ = resolve;
      });
    });
    await page.getByTestId("course-refresh").click();

    // Release the gate even if the visibility assertion below throws, so a
    // failed expectation here can't leave the stub's invoke hung.
    try {
      await expect(page.getByTestId("course-revalidating")).toBeVisible();
    } finally {
      await page.evaluate(() => window.__releaseCourse__());
    }

    await expect(page.getByTestId("course-revalidating")).toBeHidden();
    await expect(page.getByTestId("course-freshness")).toBeVisible();
    expect(await fetchCount(page, testInfo.workerIndex)).toBe(2);
  });
});
