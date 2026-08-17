import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const seed = `;(() => {
  window.__languages__ = [{ code: "en", title: "English", known_words: 500 }];
  window.__collections__ = [{ id: 7, title: "Course A" }];
})();`;

test.describe("one-shot upload error surface", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(seed);
  });

  test("a failed upload after progress events shows the error, not a stalled bar", async ({
    page,
  }) => {
    await page.goto("/upload");

    await page.locator("select").first().selectOption("en");
    await page.locator("select").nth(1).selectOption("7");

    // Pick text then audio via the dialog stub. Exact names avoid matching
    // the "Add the audio" submit-label button, which also contains "audio".
    await page.evaluate(() => (window.__dialogPickPath__ = "/tmp/ch1.xhtml"));
    await page
      .getByRole("button", { name: "Drop chapter text or click to choose" })
      .click();
    await page.evaluate(() => (window.__dialogPickPath__ = "/tmp/ch1.mp3"));
    await page
      .getByRole("button", { name: "Drop audio or click to choose" })
      .click();

    // Gate the upload so we can emit progress mid-flight, then fail it.
    await page.evaluate(() => {
      window.__uploadOneShotError__ = {
        kind: "Lingq",
        message: { kind: "Server", message: "boom" },
      };
      window.__uploadOneShotGate__ = new Promise((r) => {
        window.__releaseUpload__ = r;
      });
    });

    await page.getByRole("button", { name: "Upload lesson" }).click();
    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "Started",
        job_id: "job-1",
        stage: { kind: "transcoding" },
      }),
    );
    await expect(page.getByText("Transcoding audio")).toBeVisible();

    await page.evaluate(() => window.__releaseUpload__?.());

    // The bug: ProgressPanel stays mounted and this alert never appears.
    await expect(page.getByRole("alert")).toContainText(/LingQ/i);
    await expect(
      page.getByRole("button", { name: "Upload lesson" }),
    ).toBeVisible();
  });

  test("Cancel during upload invokes cmd_cancel_job", async ({ page }) => {
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
      window.__uploadOneShotGate__ = new Promise((r) => {
        window.__releaseUpload__ = r;
      });
    });

    await page.getByRole("button", { name: "Upload lesson" }).click();
    await page.evaluate(() =>
      window.__emitEvent__("job", {
        kind: "Started",
        job_id: "job-9",
        stage: { kind: "transcoding" },
      }),
    );

    await page.getByRole("button", { name: "Cancel" }).click();
    const logged = await page.evaluate(() =>
      window.__invokeLog__.includes("cmd_cancel_job"),
    );
    expect(logged).toBe(true);

    await page.evaluate(() => window.__releaseUpload__?.());
  });
});
