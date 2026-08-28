import { expect, test } from "./setup/test";

test.describe("quick upload promotion", () => {
  test("header nav reaches the one-shot upload", async ({ page }) => {
    await page.goto("/library");
    const toolbar = page.getByTestId("app-toolbar");
    await toolbar.getByRole("link", { name: "Quick upload" }).click();
    await expect(page).toHaveURL(/\/upload$/);
    await expect(
      toolbar.getByRole("heading", { name: "Quick upload" }),
    ).toBeVisible();
  });

  test("settings no longer buries the link", async ({ page }) => {
    await page.goto("/settings");
    await expect(page.getByText("One-shot upload (legacy)")).toHaveCount(0);
  });
});
