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

  test("accent fills resolve to a real colour, not a literal token", async ({
    page,
  }) => {
    const probe = await page.evaluate(() => {
      const el = document.createElement("button");
      el.className = "bg-accent text-accent-fg";
      document.body.append(el);
      const cs = getComputedStyle(el);
      const out = { bg: cs.backgroundColor, fg: cs.color };
      el.remove();
      return out;
    });
    expect(probe.bg).toMatch(/^rgba?\(/);
    expect(probe.bg).not.toBe("rgba(0, 0, 0, 0)");
    expect(probe.bg).not.toBe("rgb(79, 70, 229)"); // the old hardcoded indigo
    expect(probe.fg).toMatch(/^rgba?\(/);
    expect(probe.fg).not.toBe(probe.bg);
  });
});
