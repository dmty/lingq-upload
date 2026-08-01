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
