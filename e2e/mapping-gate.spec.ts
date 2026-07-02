import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const PROJECT_KEY = "gate-fixture";

function fixtureScript(): string {
  const chapters = Array.from({ length: 5 }, (_, i) => ({
    id: `idx:${i}`,
    order: i,
    title: `Chapter ${i + 1}`,
    body: "x".repeat(100),
    kind: "body",
  }));
  const pair = (i: number, t: string, conf: number) => ({
    chapter_id: `idx:${i}`,
    track_id: t,
    confidence: conf,
    touched: false,
    original_confidence: conf,
  });
  const mapping = {
    pairs: [pair(0, "t0", 1), pair(1, "t0", 1), pair(2, "t0", 1), pair(3, "t1", 0.3), pair(4, "t1", 1)],
    parking_lot: [],
    op_id: 0,
    buckets: [
      { trackId: "t0", atomTitle: "Audio 1", atomDurationSec: 600, charsPerSec: 5, audioPath: "/x/a0.m4b", window: null },
      { trackId: "t1", atomTitle: "Audio 2", atomDurationSec: 300, charsPerSec: 5, audioPath: "/x/a1.m4b", window: null },
    ],
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = null;
    window.__mappingState__.seed(${JSON.stringify(PROJECT_KEY)}, ${JSON.stringify(mapping)});
  })();`;
}

test.describe("mapping grid gate + arrows", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("gate reason is inline and clears on confirm", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-continue")).toBeDisabled();
    await expect(page.getByTestId("continue-blockers")).toContainText("1 low-confidence pair needs review");

    await page.getByTestId("confirm-pair").click();
    await expect(page.getByTestId("mapping-continue")).toBeEnabled();
    await expect(page.getByTestId("continue-blockers")).toHaveCount(0);
  });

  test("every row renders both arrows; only legal ones are enabled", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("chapter-move-up")).toHaveCount(5);
    await expect(page.getByTestId("chapter-move-down")).toHaveCount(5);

    // Mid-band row (Chapter 2) — both disabled.
    await expect(page.getByTestId("chapter-move-up").nth(1)).toBeDisabled();
    await expect(page.getByTestId("chapter-move-down").nth(1)).toBeDisabled();
    // First row of band 2 (Chapter 4) can move up.
    await expect(page.getByTestId("chapter-move-up").nth(3)).toBeEnabled();
  });
});
