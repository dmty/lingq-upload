import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const PROJECT_KEY = "steps-fixture";

function fixtureScript(): string {
  const chapters = [
    { id: "idx:0", order: 0, title: "Chapter 1", body: "x".repeat(100), kind: "body" },
  ];
  const mapping = {
    pairs: [
      { chapter_id: "idx:0", track_id: "t0", confidence: 1, touched: false, original_confidence: 1 },
    ],
    parking_lot: [],
    op_id: 0,
    buckets: [
      { trackId: "t0", atomTitle: "Audio 1", atomDurationSec: 600, charsPerSec: 5, audioPath: "/x/a0.m4b", window: null },
    ],
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = null;
    window.__mappingState__.seed(${JSON.stringify(PROJECT_KEY)}, ${JSON.stringify(mapping)});
  })();`;
}

test.describe("pipeline step indicator", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("add page marks step 1 current", async ({ page }) => {
    await page.goto("/add");
    const indicator = page.getByTestId("step-indicator");
    await expect(indicator).toBeVisible();
    await expect(indicator.locator('[aria-current="step"]')).toContainText("Add");
  });

  test("match page marks step 2 current", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    const indicator = page.getByTestId("step-indicator");
    await expect(indicator.locator('[aria-current="step"]')).toContainText("Match");
  });
});
