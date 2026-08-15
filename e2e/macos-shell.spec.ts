import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

test.describe("macOS shell tokens", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
  });

  test("body sits on the macOS 13px base size", async ({ page }) => {
    const size = await page.evaluate(
      () => getComputedStyle(document.body).fontSize,
    );
    expect(size).toBe("13px");
  });

  test("the accent is no longer the hardcoded indigo", async ({ page }) => {
    const accent = await page.evaluate(() =>
      getComputedStyle(document.documentElement)
        .getPropertyValue("--color-accent")
        .trim(),
    );
    expect(accent).not.toBe("#4f46e5");
    expect(accent).not.toBe("");
  });

  test("accent fills declare their own foreground token", async ({ page }) => {
    const fg = await page.evaluate(() =>
      getComputedStyle(document.documentElement)
        .getPropertyValue("--color-accent-fg")
        .trim(),
    );
    expect(fg).not.toBe("");
  });
});
