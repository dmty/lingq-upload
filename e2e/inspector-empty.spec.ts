import { expect, test } from "./setup/test";

const PROJECT_KEY = "inspector-empty-fixture";

function fixtureScript(): string {
  const chapters = Array.from({ length: 2 }, (_, i) => ({
    id: `idx:${i}`,
    order: i,
    title: `Chapter ${i + 1}`,
    body: "Lorem ipsum ".repeat(20),
    kind: "body",
  }));
  const pair = (i: number) => ({
    chapter_id: `idx:${i}`,
    track_id: "t0",
    confidence: 1,
    touched: false,
    original_confidence: 1,
  });
  const mapping = {
    pairs: [pair(0), pair(1)],
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
    ],
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = null;
    window.__mappingState__.seed(${JSON.stringify(PROJECT_KEY)}, ${JSON.stringify(mapping)});
  })();`;
}

test.describe("inspector empty state", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(fixtureScript());
  });

  test("prompt before selection, inspector after", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("chapter-inspector-empty")).toContainText(
      /select a chapter/i,
    );

    await page.getByTestId("mapping-chapter-row").first().click();
    await expect(page.getByTestId("chapter-inspector")).toBeVisible();
    await expect(page.getByTestId("chapter-inspector-empty")).toHaveCount(0);
  });
});
