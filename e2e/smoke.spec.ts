import { expect, test } from "./setup/test";

// Baseline harness check. If this spec goes red the entire e2e suite is
// dead — Vite isn't booting, the Tauri stub isn't installing, or the
// Library route stopped mounting. Do not skip.
test.describe("smoke", () => {
  test("app boots and Library route mounts", async ({ page }) => {
    await page.goto("/library");
    await expect(page.locator("body")).toBeVisible();
    await expect(page.getByRole("heading", { name: "Library" })).toBeVisible({
      timeout: 5_000,
    });
  });
});
