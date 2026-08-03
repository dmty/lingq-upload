import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";
import { runFixtureScript } from "./setup/run-fixture";

const KEY = "run-fixture";

// No plan hook: this spec covers the receipts-only fallback path.
const projectScript = () =>
  runFixtureScript({
    key: KEY,
    title: "Run Fixture",
    receipts: [{ chapter_index: 0 }, { chapter_index: 1 }, { chapter_index: 2 }],
  });

test.describe("run completion and cancel states", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(projectScript());
  });

  test("chapter counter, completion banner, course link", async ({ page }) => {
    await page.goto(`/run/${KEY}`);
    await page.getByRole("button", { name: "Resume" }).click();

    await page.evaluate(() =>
      window.__emitEvent__("job", { kind: "Started", job_id: "job-1", stage: { kind: "uploading" } }),
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

    await page.evaluate(() =>
      window.__emitEvent__("job", { kind: "Result", job_id: "job-1", ok: true, payload: null }),
    );
    await expect(page.getByTestId("run-complete")).toContainText("All chapters uploaded");
    await expect(page.getByTestId("run-complete")).toContainText("View Course");
  });

  test("Cancel shows a pending state until Cancelled arrives", async ({ page }) => {
    await page.goto(`/run/${KEY}`);
    await page.getByRole("button", { name: "Resume" }).click();
    await page.evaluate(() =>
      window.__emitEvent__("job", { kind: "Started", job_id: "job-1", stage: { kind: "uploading" } }),
    );

    await page.getByRole("button", { name: "Cancel" }).click();
    await expect(page.getByRole("button", { name: "Cancelling…" })).toBeDisabled();

    await page.evaluate(() =>
      window.__emitEvent__("job", { kind: "Cancelled", job_id: "job-1" }),
    );
    await expect(page.getByRole("button", { name: "Resume" })).toBeVisible();
  });
});

// Separate fixture: the run screen's completion banner links to
// `/course/${projectKey}`, so the route param it lands on has to be in the
// same joinKey form (`ch:<hash>`) the course screen expects — unlike the
// bare hash the specs above use for `/run` directly. The project fixture is
// keyed by that full route param (`ch:<hash>`), while the library entry
// below holds the bare hash — joinKey adds the `ch:` prefix itself, so the
// two need to differ for the two lookups to agree on the same identity.
const COURSE_LINK_KEY = "run-course-link";
const COURSE_LINK_ROUTE_KEY = encodeURIComponent(`ch:${COURSE_LINK_KEY}`);

const courseLinkProjectScript = () =>
  runFixtureScript({
    key: `ch:${COURSE_LINK_KEY}`,
    title: "Run Fixture",
    receipts: [{ chapter_index: 0 }],
    lingqCollectionId: 7,
  });

const courseLinkLibraryScript = () => `
;(() => {
  window.__libraryEntries__ = [{
    id: { content_hash: "${COURSE_LINK_KEY}", audible_asin: null, isbn13: null, calibre_uuid: null },
    title: "Run Fixture",
    language: "en",
    completed_lesson_count: 1,
    receipt_count: 1,
    mtime: null,
    authors: [],
    series: null,
    lingq_collection_id: 7,
    status: "done",
  }];
})();
`;

test.describe("run completion links to the course screen", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(courseLinkProjectScript());
    await page.addInitScript(courseLinkLibraryScript());
  });

  test("View Course opens the course screen for the finished project", async ({
    page,
  }) => {
    await page.goto(`/run/${COURSE_LINK_ROUTE_KEY}`);
    await page.getByRole("button", { name: "Resume" }).click();

    await page.evaluate(() =>
      window.__emitEvent__("job", { kind: "Started", job_id: "job-1", stage: { kind: "uploading" } }),
    );
    await page.evaluate(() =>
      window.__emitEvent__("job", { kind: "Result", job_id: "job-1", ok: true, payload: null }),
    );
    await expect(page.getByTestId("run-complete")).toBeVisible();

    await page.getByRole("button", { name: "View Course" }).click();

    await expect(page).toHaveURL(new RegExp(`/course/${COURSE_LINK_ROUTE_KEY}`));
    await expect(page.getByTestId("course-header")).toBeVisible();
  });
});
