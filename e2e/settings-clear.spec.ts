import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

test.describe("settings key clear confirm", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(`window.__lingqKey__ = "sk-test-abcd1234";`);
  });

  test("first click arms, second click clears", async ({ page }) => {
    await page.goto("/settings");
    await expect(page.getByText("•••• 1234")).toBeVisible();

    await page.getByRole("button", { name: "Clear" }).click();
    // Armed, not cleared.
    await expect(page.getByRole("button", { name: "Really clear?" })).toBeVisible();
    await expect(page.getByText("•••• 1234")).toBeVisible();

    await page.getByRole("button", { name: "Really clear?" }).click();
    await expect(page.getByText("No key set yet.")).toBeVisible();
  });
});
