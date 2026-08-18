import { expect, test } from "./setup/test";

const PROJECT_KEY = "savefail-fixture";

function fixtureScript(): string {
  const chapters = Array.from({ length: 3 }, (_, i) => ({
    id: `idx:${i}`,
    order: i,
    title: `Chapter ${i + 1}`,
    body: "x".repeat(100),
    kind: "body",
  }));
  const pair = (i: number, t: string) => ({
    chapter_id: `idx:${i}`,
    track_id: t,
    confidence: 1,
    touched: false,
    original_confidence: 1,
  });
  const mapping = {
    pairs: [pair(0, "t0"), pair(1, "t0"), pair(2, "t1")],
    parking_lot: [],
    op_id: 0,
    buckets: [
      {
        trackId: "t0",
        atomTitle: "Audio 1",
        atomDurationSec: 600,
        charsPerSec: 5,
        audioPath: "/x/a0.m4b",
        window: null,
      },
      {
        trackId: "t1",
        atomTitle: "Audio 2",
        atomDurationSec: 300,
        charsPerSec: 5,
        audioPath: "/x/a1.m4b",
        window: null,
      },
    ],
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = null;
    window.__mappingState__.seed(${JSON.stringify(PROJECT_KEY)}, ${JSON.stringify(mapping)});
  })();`;
}

test.describe("mapping save failure notice", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(fixtureScript());
  });

  test("a rejected op shows a transient footer notice", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(3);

    await page.evaluate(() => (window.__failNextMappingOp__ = true));
    // Chapter 3 (first row of band t1) can move up — enabled arrow.
    await page.getByTestId("chapter-move-up").nth(2).click();

    // Debounced flush is 500ms; the notice then appears.
    await expect(page.getByTestId("mapping-saved-label")).toContainText(
      "Couldn't save — change reverted",
      { timeout: 3000 },
    );
  });
});
