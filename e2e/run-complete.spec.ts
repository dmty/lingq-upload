import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const KEY = "run-fixture";

function projectScript(): string {
  const receipt = (i: number) => ({
    chapter_index: i,
    lesson_id: null,
    uploaded_at: null,
    degraded: false,
  });
  const project = {
    schema_version: 1,
    id: { content_hash: KEY, audible_asin: null, isbn13: null, calibre_uuid: null },
    sources: { text: null, audio: null },
    settings: { language: "en", collection_title: "Run Fixture", level: 1, tags: [] },
    receipts: [receipt(0), receipt(1), receipt(2)],
    queue_cursor: 0,
    completed_lesson_ids: [],
    matcher_decision: null,
    cover_path: null,
    authors: [],
    series: null,
    lingq_collection_id: 42,
    last_activity_at: null,
    stage: "mapped",
    last_transition_at: null,
    skipped_chapters: [],
    mapping: null,
    confirmed_at: "2026-01-01T00:00:00Z",
  };
  return `window.__projectByKey__ = { ${JSON.stringify(KEY)}: ${JSON.stringify(project)} };`;
}

test.describe("run completion and cancel states", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(projectScript());
  });

  test("chapter counter, completion banner, LingQ link", async ({ page }) => {
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
    await expect(page.getByTestId("run-complete")).toContainText("Open in LingQ");
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
