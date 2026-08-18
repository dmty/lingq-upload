import { expect, seed, test } from "./setup/test";
import { runFixture } from "./setup/run-fixture";

const KEY = "seed-fixture";

// A fresh confirmed project: a plan exists, but nothing has uploaded yet.
// This is the state the old code rendered as "Press Start" while running,
// with a 1/1 counter once the first chapter landed.
const fixture = () =>
  runFixture({
    key: KEY,
    title: "Seed Fixture",
    plan: [
      { chapter_index: 0, title: "The Wind on the Heath" },
      { chapter_index: 1, title: "A Night in Sussex" },
      { chapter_index: 2, title: "Bonus Track", degraded: true },
    ],
  });

test.describe("run queue seeding", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, fixture());
  });

  test("shows the whole queue with real titles before any upload", async ({
    page,
  }) => {
    await page.goto(`/run/${KEY}`);

    await expect(page.getByText("The Wind on the Heath")).toBeVisible();
    await expect(page.getByText("A Night in Sussex")).toBeVisible();
    await expect(page.getByText("Bonus Track")).toBeVisible();
    await expect(
      page.getByTestId("chapter-rows").getByRole("listitem"),
    ).toHaveCount(3);
  });

  test("counter denominator is the full queue, not the completed count", async ({
    page,
  }) => {
    await page.goto(`/run/${KEY}`);
    await page.getByRole("button", { name: "Start" }).click();

    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "Started",
        job_id: "job-1",
        stage: { kind: "uploading" },
      }),
    );
    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "ChapterDone",
        job_id: "job-1",
        chapter_index: 0,
        lesson_id: 900,
        degraded: false,
      }),
    );

    await expect(page.getByText("1/3")).toBeVisible();
    await expect(page.getByText("1/1")).toHaveCount(0);
  });

  // reloadProject() re-runs on every terminal event (Result, Cancelled), and
  // it re-derives rows from planSteps + receipts each time. A regression
  // that dropped the seeded plan here would collapse the queue back to the
  // receipts-only fallback, which for a project with no persisted receipts
  // means the row set — and the completed titles — vanish.
  test("seeded queue survives the completion reload", async ({ page }) => {
    await page.goto(`/run/${KEY}`);
    await page.getByRole("button", { name: "Start" }).click();

    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "Started",
        job_id: "job-1",
        stage: { kind: "uploading" },
      }),
    );
    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "ChapterDone",
        job_id: "job-1",
        chapter_index: 0,
        lesson_id: 900,
        degraded: false,
      }),
    );
    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "Result",
        job_id: "job-1",
        ok: true,
        payload: null,
      }),
    );

    await expect(page.getByTestId("run-complete")).toContainText(
      "All chapters uploaded",
    );
    await expect(page.getByText("The Wind on the Heath")).toBeVisible();
    await expect(page.getByText("A Night in Sussex")).toBeVisible();
    await expect(page.getByText("Bonus Track")).toBeVisible();
    await expect(
      page.getByTestId("chapter-rows").getByRole("listitem"),
    ).toHaveCount(3);
  });
});
