import { expect, test } from "./setup/test";

test.describe("quick upload promotion", () => {
  test("header nav reaches the one-shot upload", async ({ page }) => {
    await page.goto("/library");
    const toolbar = page.getByTestId("app-toolbar");
    await toolbar.getByRole("link", { name: "Quick upload" }).click();
    await expect(page).toHaveURL(/\/upload$/);
    await expect(toolbar.getByTestId("toolbar-title")).toBeVisible();
    await expect(toolbar.getByTestId("toolbar-title")).toHaveText(
      "Quick upload",
    );
  });

  test("settings no longer buries the link", async ({ page }) => {
    await page.goto("/settings");
    await expect(page.getByText("One-shot upload (legacy)")).toHaveCount(0);
  });
});

test.describe("global toolbar shortcuts", () => {
  test("Cmd+N, Cmd+Shift+U, Cmd+[ and Cmd+] drive the shell", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");

    await page.keyboard.press("Meta+n");
    await expect(page).toHaveURL(/\/add$/);
    await page.keyboard.press("Meta+Shift+u");
    await expect(page).toHaveURL(/\/upload$/);
    await page.keyboard.press("Meta+[");
    await expect(page).toHaveURL(/\/add$/);
    await page.keyboard.press("Meta+]");
    await expect(page).toHaveURL(/\/upload$/);
  });

  test("shortcuts are ignored while an editable control owns the event", async ({
    page,
  }) => {
    await page.goto("/settings");
    await page.waitForLoadState("networkidle");
    const input = page.locator('input[type="password"]').first();
    await input.focus();
    await page.keyboard.press("Meta+n");
    await expect(page).toHaveURL(/\/settings$/);
  });

  test("shortcuts are ignored while a modal dialog is open", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    await page.evaluate(() => {
      const dialog = document.createElement("dialog");
      dialog.id = "shortcut-test-modal";
      document.body.append(dialog);
      dialog.showModal();
    });

    await page.keyboard.press("Meta+n");
    await expect(page).toHaveURL(/\/library$/);

    await page.evaluate(() =>
      document.getElementById("shortcut-test-modal")?.remove(),
    );
  });
});
