import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

// Confirming a `Split by embedded chapters` decision must seed the mapping
// grid for review, not jump straight to /run. The user gets one last look at
// the chapter ↔ track pairing before transcode kicks off.

const PROJECT_KEY = "split-resolve-fixture";
const CHAPTER_COUNT = 85;
const TRACK_COUNT = 6;

function fixtureScript(): string {
  const chapters = Array.from({ length: CHAPTER_COUNT }, (_, i) => ({
    id: `idx:${i}`,
    order: i,
    title: `Chapter ${i + 1}`,
    body: "",
    kind: "body",
  }));
  const inspection = {
    title: "Many-to-Few Fixture",
    chapter_count: CHAPTER_COUNT,
    track_count: TRACK_COUNT,
    condition: "many_to_few",
    options: ["split_proportional", "single_lesson", "cancel"],
    preselect: "split_proportional",
    bucket_preview: Array.from({ length: TRACK_COUNT }, (_, i) => ({
      atomTitle: `Atom ${i + 1}`,
      atomDurationSec: 600 + i * 30,
      textRangeStart: Math.floor((i * CHAPTER_COUNT) / TRACK_COUNT),
      textRangeEnd: Math.floor(((i + 1) * CHAPTER_COUNT) / TRACK_COUNT),
      charsPerSec: 12.0,
    })),
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || {
      skippedByProject: {},
      chaptersByProject: {},
    };
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = ${JSON.stringify(inspection)};
  })();`;
}

test.describe("match resolve transitions to mapping grid", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("Split by embedded chapters seeds the grid and stays on /match", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);

    // Resolver UI is up.
    await expect(
      page.getByRole("heading", { name: "Resolve mismatch" }),
    ).toBeVisible();

    // SplitProportional is the preselect for ManyToFew. Confirm it.
    page.on("dialog", (d) => void d.accept());
    await page.getByRole("button", { name: "Confirm" }).click();

    // Stayed on /match — did NOT jump to /run.
    await expect(page).toHaveURL(new RegExp(`/match/${PROJECT_KEY}$`));

    // Grid rendered.
    await expect(page.getByTestId("mapping-grid")).toBeVisible({
      timeout: 5_000,
    });

    // 85 chapter rows, 6 distinct track rows.
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(
      CHAPTER_COUNT,
    );
    await expect(page.getByTestId("mapping-track-row")).toHaveCount(
      TRACK_COUNT,
    );
  });
});
