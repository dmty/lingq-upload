import { expect, seed, test } from "./setup/test";

test.describe("settings key clear confirm", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, { __lingqKey__: "sk-test-abcd1234" });
  });

  test("a stored key shows itself masked, and Remove asks once", async ({
    page,
  }) => {
    await page.goto("/settings");
    const field = page.getByLabel("Key", { exact: true });
    await expect(field).toHaveValue(/1234$/);
    await expect(field).toHaveAttribute("readonly", "");

    await page.getByRole("button", { name: "Remove", exact: true }).click();
    // Armed, not cleared.
    await expect(
      page.getByRole("button", { name: "Really clear?" }),
    ).toBeVisible();
    await expect(field).toHaveValue(/1234$/);

    await page.getByRole("button", { name: "Really clear?" }).click();
    await expect(page.getByText("No key set yet.")).toBeVisible();
    await expect(field).not.toHaveAttribute("readonly", "");
  });

  test("Change… swaps the stored key for an editable field", async ({
    page,
  }) => {
    await page.goto("/settings");
    await page.getByRole("button", { name: "Change…" }).click();

    const field = page.getByLabel("Key", { exact: true });
    await expect(field).toHaveValue("");
    await expect(field).not.toHaveAttribute("readonly", "");
    await expect(
      page.getByRole("button", { name: "Save", exact: true }),
    ).toBeDisabled();

    await page.getByRole("button", { name: "Cancel" }).click();
    await expect(field).toHaveValue(/1234$/);
  });
});
