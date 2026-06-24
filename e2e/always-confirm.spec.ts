import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

test.describe("always-confirm flow", () => {
  test("library badges unconfirmed project as Needs review", async ({
    page,
  }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(`;(() => {
      window.__libraryEntries__ = [{
        id: { content_hash: "proj-unconfirmed", audible_asin: null, isbn13: null, calibre_uuid: null },
        title: "Unconfirmed Book",
        language: "en",
        completed_lesson_count: 0,
        receipt_count: 0,
        mtime: null,
        cover_path: null,
        authors: [],
        series: null,
        lingq_collection_id: null,
        last_activity_at: null,
        status: "needs_match",
        failed_reason: null,
      }];
    })();`);

    await page.goto("/library");
    await expect(page.getByText("Needs review")).toBeVisible();
  });

  test("/run hides Start when confirmed_at is null, shows it when set", async ({
    page,
  }, testInfo) => {
    const baseProject = {
      schema_version: 1,
      id: { content_hash: "proj-guard", audible_asin: null, isbn13: null, calibre_uuid: null },
      sources: { text: null, audio: null },
      settings: { language: "en", collection_title: "Guard Book", level: 1, tags: [] },
      receipts: [],
      queue_cursor: 0,
      completed_lesson_ids: [],
      matcher_decision: null,
      cover_path: null,
      authors: [],
      series: null,
      lingq_collection_id: null,
      last_activity_at: null,
      stage: "mapped",
      last_transition_at: null,
      skipped_chapters: [],
      mapping: null,
    };

    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(`;(() => {
      window.__projectByKey__ = {
        "proj-guard": ${JSON.stringify({ ...baseProject, confirmed_at: null })},
      };
    })();`);

    await page.goto("/run/proj-guard");
    await expect(page.getByRole("button", { name: "Start" })).toHaveCount(0);

    // Re-navigate with confirmed_at set — Start must appear.
    await page.addInitScript(`;(() => {
      window.__projectByKey__ = {
        "proj-guard": ${JSON.stringify({ ...baseProject, confirmed_at: "2026-01-01T00:00:00Z" })},
      };
    })();`);
    await page.goto("/run/proj-guard");
    await expect(page.getByRole("button", { name: "Start" })).toBeVisible();
  });
});
