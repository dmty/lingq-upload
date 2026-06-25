import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const PROJECT_KEY = "bucket-fixture";

function fixtureScript(): string {
  const chapters = Array.from({ length: 5 }, (_, i) => ({
    id: `idx:${i}`,
    order: i,
    title: `Chapter ${i + 1}`,
    body: "x".repeat(100),
    kind: "body",
  }));
  const mapping = {
    pairs: [
      {
        chapter_id: "idx:0",
        track_id: "t0",
        confidence: 1,
        touched: false,
        original_confidence: 1,
      },
      {
        chapter_id: "idx:1",
        track_id: "t0",
        confidence: 1,
        touched: false,
        original_confidence: 1,
      },
      {
        chapter_id: "idx:2",
        track_id: "t0",
        confidence: 1,
        touched: false,
        original_confidence: 1,
      },
      {
        chapter_id: "idx:3",
        track_id: "t1",
        confidence: 1,
        touched: false,
        original_confidence: 1,
      },
      {
        chapter_id: "idx:4",
        track_id: "t1",
        confidence: 1,
        touched: false,
        original_confidence: 1,
      },
    ],
    parking_lot: [],
    op_id: 0,
    buckets: [
      {
        trackId: "t0",
        atomTitle: "Audio 1",
        atomDurationSec: 600,
        charsPerSec: 5,
        audioPath: "/audio/t0.m4a",
        window: [0, 600],
      },
      {
        trackId: "t1",
        atomTitle: "Audio 2",
        atomDurationSec: 300,
        charsPerSec: 5,
        audioPath: "/audio/t1.m4a",
        window: [0, 300],
      },
    ],
  };
  const inspection = {
    title: "Bucket Fixture",
    chapter_count: 5,
    track_count: 2,
    condition: "many_to_few" as const,
    options: ["split_proportional", "cancel"] as const,
    preselect: "split_proportional" as const,
    bucket_preview: null,
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = ${JSON.stringify(inspection)};
    window.__mappingState__.seed(${JSON.stringify(PROJECT_KEY)}, ${JSON.stringify(mapping)});
  })();`;
}

test.describe("chapter inspector", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("selecting a row shows the chapter text in the inspector", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("chapter-inspector")).toHaveCount(0); // nothing selected yet

    await page.getByTestId("mapping-chapter-row").nth(0).click();
    await expect(page.getByTestId("chapter-inspector")).toBeVisible();
    await expect(page.getByTestId("inspector-text")).toContainText("x"); // body is "x".repeat(100)
  });

  test("removing the selected chapter advances inspector to the next chapter", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await page.getByTestId("mapping-chapter-row").nth(1).click();
    await expect(page.getByTestId("chapter-inspector")).toContainText(
      "Chapter 2",
    );
    await page.getByTestId("inspector-remove").click();
    await expect(page.getByTestId("chapter-inspector")).toContainText(
      "Chapter 3",
    );
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(4);
  });

  test("removing the last chapter falls back to the previous one", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await page.getByTestId("mapping-chapter-row").nth(4).click();
    await expect(page.getByTestId("chapter-inspector")).toContainText(
      "Chapter 5",
    );
    await page.getByTestId("inspector-remove").click();
    await expect(page.getByTestId("chapter-inspector")).toContainText(
      "Chapter 4",
    );
  });

  test("inspector renders a windowed audio element for the bucket", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await page.getByTestId("mapping-chapter-row").nth(0).click();
    // The native <audio> is driven by a custom transport, so it is hidden;
    // assert it carries the window bounds and that the play control shows.
    const audio = page.getByTestId("inspector-audio");
    await expect(audio).toHaveAttribute("data-window-start", /\d/);
    await expect(audio).toHaveAttribute("data-window-end", /\d/);
    await expect(page.getByTestId("inspector-play")).toBeVisible();
  });

  test("the ↓ arrow on a bucket's last row moves it to the next bucket", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    // band t0={0,1,2}, t1={3,4}. Only t0's last row (idx:2) shows a ↓ arrow.
    await page.getByTestId("chapter-move-down").click();
    // boundary shifts: t0 now has 2 rows, t1 has 3 — still 2 bands.
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(2);
    const secondBand = page.getByTestId("mapping-bucket-band").nth(1);
    await expect(secondBand.getByTestId("mapping-chapter-row")).toHaveCount(3);
  });

  test("the ↑ arrow on a bucket's first row moves it to the previous bucket", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    // Only t1's first row (idx:3) shows a ↑ arrow.
    await page.getByTestId("chapter-move-up").click();
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(2);
    const firstBand = page.getByTestId("mapping-bucket-band").nth(0);
    await expect(firstBand.getByTestId("mapping-chapter-row")).toHaveCount(4);
  });

  test("interior chapters show no move arrows", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    // 2 bands → exactly one ↓ (t0 last) and one ↑ (t1 first); interior rows have none.
    await expect(page.getByTestId("chapter-move-down")).toHaveCount(1);
    await expect(page.getByTestId("chapter-move-up")).toHaveCount(1);
  });

  test("remove from the inspector drops the chapter", async ({ page }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await page.getByTestId("mapping-chapter-row").nth(0).click();
    await page.getByTestId("inspector-remove").click();
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(4);
    await expect(page.getByTestId("removed-strip")).toContainText("1");
  });
});
