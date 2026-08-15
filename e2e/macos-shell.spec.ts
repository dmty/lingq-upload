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

test.describe("source list sidebar", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("all four sections live in a labelled sidebar nav", async ({ page }) => {
    await page.goto("/library");
    const nav = page.getByRole("navigation", { name: "Sections" });
    await expect(nav).toBeVisible();
    for (const name of ["Library", "Add", "Quick upload", "Settings"]) {
      await expect(nav.getByRole("link", { name, exact: true })).toBeVisible();
    }
  });

  test("the sidebar sits beside the content, not above it", async ({ page }) => {
    await page.goto("/library");
    const nav = await page
      .getByRole("navigation", { name: "Sections" })
      .boundingBox();
    const main = await page.locator("main").boundingBox();
    expect(nav).not.toBeNull();
    expect(main).not.toBeNull();
    expect(main!.x).toBeGreaterThanOrEqual(nav!.x + nav!.width);
  });

  test("the current section is marked for assistive tech", async ({ page }) => {
    await page.goto("/settings");
    const nav = page.getByRole("navigation", { name: "Sections" });
    await expect(nav.getByRole("link", { name: "Settings", exact: true })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(nav.getByRole("link", { name: "Library", exact: true })).not.toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  test("content scrolls while the sidebar stays put", async ({ page }) => {
    await page.goto("/settings");
    await page.waitForLoadState("networkidle");
    const nav = page.getByRole("navigation", { name: "Sections" });
    const before = await nav.boundingBox();
    await page.locator("main").evaluate((el) => {
      el.style.height = "200px";
      el.scrollTop = 400;
    });
    const scrollTop = await page.locator("main").evaluate((el) => el.scrollTop);
    expect(scrollTop).toBeGreaterThan(0);
    const after = await nav.boundingBox();
    expect(before).not.toBeNull();
    expect(after!.y).toBe(before!.y);
  });
});

test.describe("overlay titlebar", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("a drag strip reserves room above the sidebar sections", async ({
    page,
  }) => {
    await page.goto("/library");
    const strip = page.locator("[data-tauri-drag-region]").first();
    await expect(strip).toBeVisible();
    const box = await strip.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.y).toBe(0);
    expect(box!.height).toBeGreaterThanOrEqual(44);
  });

  test("the first section row clears the traffic lights", async ({ page }) => {
    await page.goto("/library");
    const first = page
      .getByRole("navigation", { name: "Sections" })
      .getByRole("link", { name: "Library", exact: true });
    const box = await first.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.y).toBeGreaterThanOrEqual(44);
  });

  test("the document is transparent so vibrancy can show through", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const bg = await page.evaluate(
      () => getComputedStyle(document.body).backgroundColor,
    );
    expect(bg).toBe("rgba(0, 0, 0, 0)");
  });
});
