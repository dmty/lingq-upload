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
      () => getComputedStyle(document.body).backgroundColor,
    );
    expect(canvas).toBe("rgb(22, 21, 19)"); // --color-canvas dark
    const fg = await page.evaluate(
      () => getComputedStyle(document.body).color,
    );
    expect(fg).toBe("rgb(233, 231, 226)"); // --color-fg dark
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
      () => getComputedStyle(document.body).backgroundColor,
    );
    expect(backgroundColor).toBe("rgb(252, 252, 250)"); // #fcfcfa
  });
});
