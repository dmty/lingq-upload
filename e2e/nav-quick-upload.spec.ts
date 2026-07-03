import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

test.describe("quick upload promotion", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("header nav reaches the one-shot upload", async ({ page }) => {
    await page.goto("/library");
    await page.getByRole("link", { name: "Quick upload" }).click();
    await expect(page).toHaveURL(/\/upload$/);
    await expect(page.getByRole("heading", { name: "Quick upload" })).toBeVisible();
  });

  test("settings no longer buries the link", async ({ page }) => {
    await page.goto("/settings");
    await expect(page.getByText("One-shot upload (legacy)")).toHaveCount(0);
  });
});
