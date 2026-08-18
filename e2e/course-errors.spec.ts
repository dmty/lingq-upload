import { expect, seed, test } from "./setup/test";
import { libraryEntry } from "./setup/library-fixture";

const KEY = "course-errors";
// The route resolves its param through joinKey, same as the other course
// specs: this fixture has no asin/isbn/uuid, so joinKey falls back to the
// `ch:<hash>` form.
const ROUTE_KEY = encodeURIComponent(`ch:${KEY}`);

const entry = libraryEntry(KEY, {
  title: "Broken Course",
  language: "ja",
  completed_lesson_count: 1,
  receipt_count: 1,
  lingq_collection_id: 7,
});

const fetchCount = (
  page: import("@playwright/test").Page,
  workerIndex: number,
) =>
  page.evaluate(
    (ns) => Number(sessionStorage.getItem("__courseFetchCount__:" + ns) || "0"),
    String(workerIndex),
  );

test.describe("course screen failures", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, { __libraryEntries__: [entry] });
  });

  test("a missing API key points at Settings and keeps the LingQ button", async ({
    page,
  }) => {
    await seed(page, { __courseError__: { kind: "MissingApiKey" } });
    await page.goto(`/course/${ROUTE_KEY}`);

    await expect(page.getByTestId("course-alert")).toContainText("API key");
    await expect(page.getByTestId("course-alert")).toContainText("Settings");
    await expect(page.getByTestId("course-refresh")).toHaveCount(0);
    await expect(page.getByTestId("open-in-lingq")).toBeEnabled();
  });

  test("a transport failure offers a retry", async ({ page }) => {
    await seed(page, {
      __courseError__: {
        kind: "Lingq",
        message: { kind: "Transport", message: "dns" },
      },
    });
    await page.goto(`/course/${ROUTE_KEY}`);

    await expect(page.getByTestId("course-alert")).toContainText(
      "Couldn't reach LingQ",
    );
    await expect(page.getByTestId("course-refresh")).toBeVisible();
    await expect(page.getByTestId("open-in-lingq")).toBeEnabled();
  });

  test("a deleted course says so and offers no retry", async ({ page }) => {
    await seed(page, {
      __courseError__: { kind: "Lingq", message: { kind: "NotFound" } },
    });
    await page.goto(`/course/${ROUTE_KEY}`);

    await expect(page.getByTestId("course-alert")).toContainText(
      "no longer on LingQ",
    );
    await expect(page.getByTestId("course-refresh")).toHaveCount(0);
    await expect(page.getByTestId("open-in-lingq")).toBeEnabled();
  });

  test("a failed refresh keeps the cached numbers", async ({ page }) => {
    await seed(page, {
      __courseView__: {
        collection: {
          id: 7,
          title: "Broken Course",
          description: null,
          level: null,
          duration: 600,
          lessons_count: 3,
          new_words_count: 10,
          image_url: null,
          status: "private",
          roses_count: null,
          views_count: null,
        },
        lessons: [],
      },
    });
    await page.goto(`/course/${ROUTE_KEY}`);
    await expect(page.getByTestId("stat-lessons")).toContainText("3");

    await page.evaluate(() => {
      window.__courseError__ = {
        kind: "Lingq",
        message: { kind: "Transport", message: "dns" },
      };
    });
    await page.getByTestId("course-refresh").click();

    await expect(page.getByTestId("course-refresh-failed")).toBeVisible();
    await expect(page.getByTestId("stat-lessons")).toContainText("3");
    await expect(page.getByTestId("course-alert")).toHaveCount(0);
  });

  test("a thrown transport error still leaves Refresh able to retry", async ({
    page,
  }, testInfo) => {
    // Unlike the fixtures above (plain objects the stub throws, mirroring an
    // app-level AppError), this is a real Error instance — the shape the
    // generated binding rethrows instead of returning as a result.
    await page.addInitScript(() => {
      window.__courseError__ = new Error("socket reset");
    });
    await page.goto(`/course/${ROUTE_KEY}`);

    await expect(page.getByTestId("course-alert")).toBeVisible();
    expect(await fetchCount(page, testInfo.workerIndex)).toBe(1);

    await page.getByTestId("course-refresh").click();

    await expect.poll(() => fetchCount(page, testInfo.workerIndex)).toBe(2);
  });

  test("a course with no LingQ collection says so instead of loading forever", async ({
    page,
  }) => {
    const unuploadedKey = "course-errors-unuploaded";
    const routeKey = encodeURIComponent(`ch:${unuploadedKey}`);
    await seed(page, {
      __libraryEntries__: [
        libraryEntry(unuploadedKey, {
          title: "Not Yet Uploaded",
          language: "ja",
          status: "idle",
        }),
      ],
    });
    await page.goto(`/course/${routeKey}`);

    await expect(page.getByTestId("course-not-uploaded")).toBeVisible();
    await expect(page.getByTestId("stat-lessons")).toHaveCount(0);
    await expect(page.getByTestId("open-in-lingq")).toHaveCount(0);
  });

  test("a course with no lessons yet does not leave an empty lesson header", async ({
    page,
  }) => {
    const emptyKey = "course-errors-empty";
    const routeKey = encodeURIComponent(`ch:${emptyKey}`);
    await seed(page, {
      __libraryEntries__: [
        libraryEntry(emptyKey, {
          title: "Freshly Uploaded",
          language: "ja",
          lingq_collection_id: 9,
        }),
      ],
      __courseView__: {
        collection: {
          id: 9,
          title: "Freshly Uploaded",
          description: null,
          level: null,
          duration: 0,
          lessons_count: 0,
          new_words_count: 0,
          image_url: null,
          status: "private",
          roses_count: null,
          views_count: null,
        },
        lessons: [],
      },
    });
    await page.goto(`/course/${routeKey}`);

    await expect(page.getByTestId("course-stat-band")).toBeVisible();
    await expect(page.getByTestId("course-lessons")).toHaveCount(0);
  });

  test("a cold deep link waits for the library before calling the course missing", async ({
    page,
  }) => {
    // A pending promise and its resolver aren't serializable, so this stays
    // a plain init script rather than a seed() call.
    await page.addInitScript(() => {
      window.__libraryGate__ = new Promise((resolve) => {
        window.__releaseLibrary__ = resolve;
      });
    });
    await page.goto(`/course/${ROUTE_KEY}`);

    // Anchors the timing: proves the loading branch actually rendered before
    // the library resolved, rather than the not-found check below passing
    // vacuously because nothing had rendered yet.
    await expect(page.getByTestId("course-loading")).toBeVisible();
    await expect(page.getByTestId("course-not-found")).toHaveCount(0);

    await page.evaluate(() => window.__releaseLibrary__?.());

    await expect(page.getByTestId("course-header")).toBeVisible();
  });

  test("a library read failure says so instead of calling the course missing", async ({
    page,
  }) => {
    await seed(page, {
      __libraryError__: { kind: "Io", message: "disk unreadable" },
    });
    await page.goto(`/course/${ROUTE_KEY}`);

    await expect(page.getByTestId("course-library-error")).toBeVisible();
    await expect(page.getByTestId("course-not-found")).toHaveCount(0);
  });

  test("a route key with no matching entry says the course isn't in your library", async ({
    page,
  }) => {
    await seed(page, { __libraryEntries__: [] });
    await page.goto(`/course/${ROUTE_KEY}`);

    await expect(page.getByTestId("course-not-found")).toBeVisible();
  });
});
