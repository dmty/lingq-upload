import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

test.describe("dark color scheme", () => {
  test.use({ colorScheme: "dark" });

  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("canvas and surface tokens flip dark", async ({ page }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const canvas = await page.evaluate(
      () => getComputedStyle(document.querySelector("main")!).backgroundColor,
    );
    expect(canvas).toBe("rgb(30, 30, 30)"); // --color-canvas dark #1e1e1e
    const fg = await page.evaluate(() => getComputedStyle(document.body).color);
    expect(fg).toBe("rgb(245, 245, 247)"); // --color-fg dark #f5f5f7
  });
});

test.describe("light color scheme stays default", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("canvas stays light", async ({ page }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const backgroundColor = await page.evaluate(
      () => getComputedStyle(document.querySelector("main")!).backgroundColor,
    );
    expect(backgroundColor).toBe("rgb(242, 242, 247)"); // #f2f2f7
  });
});
