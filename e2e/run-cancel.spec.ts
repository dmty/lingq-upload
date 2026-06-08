import { expect, test } from "@playwright/test";

import { tauriStubInitScript } from "./setup/tauri-stub";

// The current tauri-stub returns an empty receipts list for
// `cmd_project_load` and has no event-emission seam for `job` events. Both
// tests below need:
//   1. a seeded `cmd_project_load` returning 3+ receipts with lesson_id=null
//   2. a hook to drive `JobEvent` messages (Started, ChapterDone, Cancelled)
// Bodies are written so the tests turn on by un-skipping once the stub
// grows that surface.

test.describe("run screen cancel flow", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(tauriStubInitScript);
  });

  test("Run screen shows queued chips for pre-populated receipts", async ({
    page,
  }) => {
    // TODO: enable when tauri-stub seeds receipts on cmd_project_load
    test.skip(true, "needs tauri-stub seeded-receipts fixture");

    await page.goto("/run/seeded-project");
    await page.waitForLoadState("networkidle");

    const rows = page.locator("[data-testid='chapter-row']");
    await expect(rows).toHaveCount(3);
    for (let i = 0; i < 3; i += 1) {
      await expect(rows.nth(i)).toContainText(/queued/i);
      await expect(rows.nth(i)).not.toContainText(/done/i);
    }
  });

  test("Cancel button reverts in-flight chips to queued", async ({ page }) => {
    // TODO: enable when tauri-stub emits Started / ChapterDone / Cancelled
    test.skip(true, "needs tauri-stub event-emission seam");

    await page.goto("/run/seeded-project");
    await page.waitForLoadState("networkidle");

    await page.getByRole("button", { name: /start|resume/i }).click();

    const inFlight = page.locator("[data-testid='chapter-row']", {
      hasText: /in[- ]?flight/i,
    });
    await expect(inFlight.first()).toBeVisible();

    await page.getByRole("button", { name: /cancel/i }).click();

    const rows = page.locator("[data-testid='chapter-row']");
    const count = await rows.count();
    for (let i = 0; i < count; i += 1) {
      await expect(rows.nth(i)).not.toContainText(/in[- ]?flight/i);
    }
  });
});
