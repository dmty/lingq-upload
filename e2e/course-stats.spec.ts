import { expect, seed, test } from "./setup/test";
import { libraryEntry } from "./setup/library-fixture";

const KEY = "course-fixture";
// The route reads its param as a joinKey identifier (same as /match and /run
// navigation elsewhere in the app). This fixture has no asin/isbn/uuid, so
// joinKey falls back to the content_hash form: `ch:<hash>`.
const ROUTE_KEY = encodeURIComponent(`ch:${KEY}`);

const fixture = (): Partial<Window> => ({
  __libraryEntries__: [
    libraryEntry(KEY, {
      title: "Kafka on the Shore",
      language: "ja",
      completed_lesson_count: 42,
      receipt_count: 42,
      authors: ["Haruki Murakami"],
      lingq_collection_id: 7,
    }),
  ],
  __courseView__: {
    collection: {
      id: 7,
      title: "Kafka on the Shore",
      description: null,
      level: "Intermediate 2",
      duration: 22320,
      lessons_count: 2,
      new_words_count: 9204,
      image_url: null,
      status: "private",
      roses_count: null,
      views_count: null,
    },
    lessons: [
      {
        id: 10,
        title: "The Boy Named Crow",
        duration: 512,
        word_count: 2841,
        unique_word_count: 900,
        new_words_count: 214,
        percent_completed: 100,
      },
      {
        id: 11,
        title: "Chapter Two",
        duration: 584,
        word_count: 3190,
        unique_word_count: 1010,
        new_words_count: 287,
        percent_completed: 41.5,
      },
    ],
  },
});

const MIXED_KEY = "course-fixture-mixed";
const MIXED_ROUTE_KEY = encodeURIComponent(`ch:${MIXED_KEY}`);

const mixedFixture = (): Partial<Window> => ({
  __libraryEntries__: [
    libraryEntry(MIXED_KEY, {
      title: "Norwegian Wood",
      language: "ja",
      completed_lesson_count: 2,
      receipt_count: 2,
      authors: ["Haruki Murakami"],
      lingq_collection_id: 8,
    }),
  ],
  __courseView__: {
    collection: {
      id: 8,
      title: "Norwegian Wood",
      description: null,
      level: "Intermediate 1",
      duration: 600,
      lessons_count: 2,
      new_words_count: 100,
      image_url: null,
      status: "private",
      roses_count: null,
      views_count: null,
    },
    lessons: [
      {
        id: 20,
        title: "Chapter One",
        duration: 300,
        word_count: 2841,
        unique_word_count: 800,
        new_words_count: 100,
        percent_completed: 100,
      },
      {
        id: 21,
        title: "Chapter Two",
        duration: 300,
        word_count: null,
        unique_word_count: null,
        new_words_count: null,
        percent_completed: 0,
      },
    ],
  },
});

test.describe("course screen", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, fixture());
  });

  test("the header renders from local data and the stat band fills from LingQ", async ({
    page,
  }) => {
    await page.goto(`/course/${ROUTE_KEY}`);

    await expect(
      page.getByTestId("app-toolbar").getByRole("heading", {
        name: "Kafka on the Shore",
      }),
    ).toBeVisible();
    await expect(page.getByTestId("course-header")).toContainText(
      "Haruki Murakami",
    );
    await expect(page.getByTestId("course-header")).not.toContainText(
      "Kafka on the Shore",
    );
    await expect(
      page.getByTestId("course-header").getByRole("link", {
        name: "Back to Library",
      }),
    ).toHaveCount(0);

    // Exactly one h1 survives for document semantics, and it stays exposed
    // to the accessibility tree (not just present with a degenerate box) —
    // querying through the role engine, rather than a bare `h1` selector,
    // proves that, since it would fail equally for aria-hidden or hidden.
    const bodyHeading = page
      .locator("main")
      .getByRole("heading", { name: "Kafka on the Shore" });
    await expect(bodyHeading).toHaveCount(1);
    const headingBox = await bodyHeading.boundingBox();
    expect(headingBox).not.toBeNull();
    expect(headingBox!.height).toBeLessThanOrEqual(1);

    const cover = page.getByTestId("course-header").locator(".cover-placeholder, img");
    const coverBox = await cover.boundingBox();
    expect(coverBox!.width).toBe(64);
    expect(coverBox!.height).toBe(64);

    const identity = page.getByTestId("course-identity");
    const identityBox = await identity.boundingBox();
    expect(identityBox!.x).toBeGreaterThanOrEqual(coverBox!.x + coverBox!.width);

    const openInLingq = page.getByTestId("course-header").getByRole("button", {
      name: "Open in LingQ",
    });
    await expect(openInLingq).toBeVisible();
    // getByRole's `name` matches a substring, so it would still pass against
    // the old "Open in LingQ ↗" text — toHaveAccessibleName checks the full
    // computed name, which is the actual regression the SVG swap prevents.
    await expect(openInLingq).toHaveAccessibleName("Open in LingQ");
    const openBox = await openInLingq.boundingBox();
    expect(openBox!.height).toBe(28);
    expect(openBox!.x).toBeGreaterThanOrEqual(identityBox!.x + identityBox!.width);

    // Exact match, not toContainText: the fixture's lessons_count (2) must
    // agree with its two-lesson array, or CourseStats logs a mismatch
    // warning — toContainText("2") would still pass against a stray "42".
    await expect(page.getByTestId("stat-lessons")).toHaveText("2 lessons");
    await expect(page.getByTestId("stat-words")).toContainText("6,031");
    await expect(page.getByTestId("stat-unique-words")).toContainText("1,910");
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

  test("the progress strip weights completion by word count", async ({
    page,
  }) => {
    await page.goto(`/course/${ROUTE_KEY}`);

    // 2841 words at 100% + 3190 words at 41.5% = 4164.85 of 6031 = 69%.
    await expect(page.getByTestId("course-progress")).toContainText("69%");
    await expect(page.getByTestId("course-progress")).toContainText(
      "1 of 2 read",
    );
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

  test("a lesson missing a word count falls back to the sibling average, not zero weight", async ({
    page,
  }) => {
    await seed(page, mixedFixture());
    await page.goto(`/course/${MIXED_ROUTE_KEY}`);

    // Chapter One (2841 words, 100%) and Chapter Two (no word count, 0%)
    // weight equally once the missing count falls back to the sibling
    // average: (100*2841 + 0*2841) / (2841+2841) = 50%.
    await expect(page.getByTestId("course-progress")).toContainText("50%");
  });

  test("the header prefers the LingQ cover over a local cover", async ({
    page,
  }) => {
    const imageUrl = "https://cdn.lingq.com/covers/kafka.webp";
    const png = Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
      "base64",
    );
    await page.route("https://cdn.lingq.com/**", (route) =>
      route.fulfill({ status: 200, contentType: "image/png", body: png }),
    );

    const view = fixture().__courseView__;
    await seed(page, {
      __libraryEntries__: [
        libraryEntry(KEY, {
          title: "Kafka on the Shore",
          language: "ja",
          completed_lesson_count: 42,
          receipt_count: 42,
          authors: ["Haruki Murakami"],
          lingq_collection_id: 7,
          cover_path: "/tmp/local-cover.jpg",
        }),
      ],
      __courseView__: {
        ...view!,
        collection: { ...view!.collection, image_url: imageUrl },
      },
    });
    await page.goto(`/course/${ROUTE_KEY}`);

    const img = page.getByTestId("course-header").locator("img");
    await expect(img).toHaveAttribute("src", imageUrl);
  });
});
