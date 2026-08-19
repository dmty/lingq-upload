import type { Page } from "@playwright/test";
import { expect, test } from "./setup/test";

const readSidebarWidth = (page: Page) =>
  page.evaluate(() => {
    const el = document.querySelector<HTMLElement>(".app-shell");
    const raw = el?.style.getPropertyValue("--sidebar-width") ?? "";
    return Number.parseInt(raw.replace("px", ""), 10);
  });

test.describe("sidebar toggle + resize", () => {
  test("toggle hides the sidebar and floating button restores it", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");

    const sidebar = page.locator("#app-sidebar");
    await expect(sidebar).toBeVisible();

    await page.getByTestId("sidebar-toggle").click();
    await expect(sidebar).toBeHidden();
    await expect(page.locator(".app-shell")).toHaveAttribute(
      "data-sidebar-collapsed",
      "true",
    );

    await page.getByTestId("sidebar-floating-toggle").click();
    await expect(sidebar).toBeVisible();
    await expect(page.locator(".app-shell")).toHaveAttribute(
      "data-sidebar-collapsed",
      "false",
    );
  });

  test("dragging the resize handle changes width within clamp range", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");

    expect(await readSidebarWidth(page)).toBe(220);

    const handle = page.getByTestId("sidebar-resize-handle");
    const box = await handle.boundingBox();
    if (!box) throw new Error("resize handle has no bounding box");

    // Drag right to widen to ~300 px.
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(300, box.y + box.height / 2, { steps: 10 });
    await page.mouse.up();

    expect(await readSidebarWidth(page)).toBe(300);

    // Drag far right — clamps at 400.
    const box2 = await handle.boundingBox();
    if (!box2) throw new Error("resize handle has no bounding box");
    await page.mouse.move(box2.x + box2.width / 2, box2.y + box2.height / 2);
    await page.mouse.down();
    await page.mouse.move(1200, box2.y + box2.height / 2, { steps: 10 });
    await page.mouse.up();

    expect(await readSidebarWidth(page)).toBe(400);

    // Drag far left — clamps at 180.
    const box3 = await handle.boundingBox();
    if (!box3) throw new Error("resize handle has no bounding box");
    await page.mouse.move(box3.x + box3.width / 2, box3.y + box3.height / 2);
    await page.mouse.down();
    await page.mouse.move(20, box3.y + box3.height / 2, { steps: 10 });
    await page.mouse.up();

    expect(await readSidebarWidth(page)).toBe(180);
  });

  test("width and collapsed state survive a reload", async ({ page }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");

    // Set a distinctive width via a direct store call rather than a drag —
    // faster and doesn't retest the drag math.
    await page.evaluate(() => {
      window.localStorage.setItem(
        "sidebar:v1",
        JSON.stringify({ width: 275, collapsed: false }),
      );
    });
    await page.reload();
    await page.waitForLoadState("networkidle");

    expect(await readSidebarWidth(page)).toBe(275);

    await page.getByTestId("sidebar-toggle").click();
    await expect(page.locator(".app-shell")).toHaveAttribute(
      "data-sidebar-collapsed",
      "true",
    );

    await page.reload();
    await page.waitForLoadState("networkidle");

    await expect(page.locator(".app-shell")).toHaveAttribute(
      "data-sidebar-collapsed",
      "true",
    );
    await expect(page.locator("#app-sidebar")).toBeHidden();
  });
});
