import { expect, test, type Page } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const seed = `;(() => {
  window.__languages__ = [{ code: "en", title: "English", known_words: 500 }];
  window.__collections__ = [{ id: 7, title: "Course A" }];
})();`;

async function startUpload(page: Page) {
  await page.goto("/upload");
  await page.locator("select").first().selectOption("en");
  await page.locator("select").nth(1).selectOption("7");
  await page.evaluate(() => (window.__dialogPickPath__ = "/tmp/ch1.xhtml"));
  await page
    .getByRole("button", { name: "Drop chapter text or click to choose" })
    .click();
  await page.evaluate(() => (window.__dialogPickPath__ = "/tmp/ch1.mp3"));
  await page
    .getByRole("button", { name: "Drop audio or click to choose" })
    .click();
  await page.evaluate(() => {
    window.__uploadOneShotGate__ = new Promise((resolve) => {
      window.__releaseUpload__ = resolve;
    });
  });
  await page.getByRole("button", { name: "Upload lesson" }).click();
}

test.describe("aggregate upload progress", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(seed);
  });

  test("upload bar accumulates without resetting", async ({ page }) => {
    await startUpload(page);

    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "Started",
        job_id: "j1",
        stage: { kind: "parsing" },
      }),
    );
    await expect(page.getByText("Step 1 of 3 · Reading text")).toBeVisible();
    await expect(page.getByText("0%")).toBeVisible();

    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "Progress",
        job_id: "j1",
        pct: 0.5,
        message: null,
      }),
    );
    await expect(page.getByText("17%")).toBeVisible();

    // New stage must not reset the bar to 0 — it jumps to the stage floor.
    // Real jobs emit StageChanged (not another Started) for stage 2+.
    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "StageChanged",
        job_id: "j1",
        stage: { kind: "transcoding" },
      }),
    );
    await expect(page.getByText("Step 2 of 3 · Transcoding audio")).toBeVisible();
    await expect(page.getByText("33%")).toBeVisible();

    await page.evaluate(() => window.__releaseUpload__());
  });

  test("standalone detection reports phase progress", async ({ page }) => {
    await page.goto("/upload");
    await page.waitForFunction(
      () => (window.__eventHandlers__.job?.length ?? 0) > 0,
    );

    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "Started",
        job_id: "detect-1",
        stage: { kind: "detecting_start" },
      }),
    );
    await expect(page.getByText("Step 1 of 1 · Detecting text start")).toBeVisible();
    await expect(page.getByText("0%")).toBeVisible();

    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "DetectionProgress",
        job_id: "detect-1",
        pct: 0.5,
        phase: "title_check",
      }),
    );
    await expect(page.getByText("Checking chapter titles")).toBeVisible();
    await expect(page.getByText("50%")).toBeVisible();
  });
});
