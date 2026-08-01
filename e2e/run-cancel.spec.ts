import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";
import { runFixtureScript } from "./setup/run-fixture";

const KEY = "cancel-fixture";

// Three planned chapters, none uploaded yet.
const fixtureScript = () =>
  runFixtureScript({
    key: KEY,
    title: "Cancel Fixture",
    plan: [
      { chapter_index: 0, title: "Approach to Dunwich" },
      { chapter_index: 1, title: "The Fields Beyond" },
      { chapter_index: 2, title: "Return by Night" },
    ],
  });

const rows = (page: import("@playwright/test").Page) =>
  page.locator("[data-testid='chapter-row']");

test.describe("run screen cancel flow", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("every planned chapter starts queued", async ({ page }) => {
    await page.goto(`/run/${KEY}`);

    await expect(rows(page)).toHaveCount(3);
    for (let i = 0; i < 3; i += 1) {
      await expect(rows(page).nth(i)).toHaveAttribute("data-status", "queued");
    }
  });

  test("no row claims to be in flight while inputs are still resolving", async ({
    page,
  }) => {
    await page.goto(`/run/${KEY}`);
    await page.getByRole("button", { name: "Start" }).click();

    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "Started",
        job_id: "job-1",
        stage: { kind: "parsing" },
      }),
    );

    await expect(
      page.locator("[data-testid='chapter-row'][data-status='in_flight']"),
    ).toHaveCount(0);

    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "StageChanged",
        job_id: "job-1",
        stage: { kind: "uploading" },
      }),
    );

    await expect(rows(page).nth(0)).toHaveAttribute("data-status", "in_flight");
  });

  test("the chapter being uploaded is marked in flight, and Cancel clears it", async ({
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

    // Nothing done yet, so the head of the queue is the live one.
    await expect(rows(page).nth(0)).toHaveAttribute("data-status", "in_flight");
    await expect(rows(page).nth(1)).toHaveAttribute("data-status", "queued");

    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "ChapterDone",
        job_id: "job-1",
        chapter_index: 0,
        lesson_id: 900,
        degraded: false,
      }),
    );

    // The marker advances with the queue rather than sticking to chapter 1.
    await expect(rows(page).nth(0)).toHaveAttribute("data-status", "done");
    await expect(rows(page).nth(1)).toHaveAttribute("data-status", "in_flight");

    await page.getByRole("button", { name: /cancel/i }).click();
    await page.evaluate(() =>
      window.__emitEvent__("job", { kind: "Cancelled", job_id: "job-1" }),
    );

    // Cancelled: no row may still claim to be uploading.
    await expect(
      page.locator("[data-testid='chapter-row'][data-status='in_flight']"),
    ).toHaveCount(0);
    // No receipts persisted in this fixture, so the run restarts from the top.
    await expect(page.getByRole("button", { name: "Start" })).toBeVisible();
  });
});
