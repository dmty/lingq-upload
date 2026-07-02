import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const KEY = "paused-fixture";
const PARAMS =
  "?title=Paused%20Book&chapters=5&tracks=2&condition=many_to_few" +
  "&options=split_proportional,cancel&preselect=split_proportional";

test.describe("match paused notice", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("URL-param entry (live from run) shows the paused notice", async ({ page }) => {
    await page.goto(`/match/${KEY}${PARAMS}`);
    await expect(page.getByTestId("paused-notice")).toContainText(/upload paused/i);
  });

  test("cold entry from library shows no notice", async ({ page }) => {
    await page.goto(`/match/${KEY}`);
    await expect(page.getByText(/resolve mismatch/i)).toBeVisible();
    await expect(page.getByTestId("paused-notice")).toHaveCount(0);
  });
});
