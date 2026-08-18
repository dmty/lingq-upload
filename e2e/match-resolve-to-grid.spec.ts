import type { MismatchInspection } from "../src/lib/ipc/bindings";
import { chapters, seedChapters } from "./setup/mapping-fixture";
import { expect, seed, test } from "./setup/test";

// Confirming a `Split by embedded chapters` decision must seed the mapping
// grid for review, not jump straight to /run. The user gets one last look at
// the chapter ↔ track pairing before transcode kicks off.

const PROJECT_KEY = "split-resolve-fixture";
const CHAPTER_COUNT = 85;
const TRACK_COUNT = 6;

const inspection: MismatchInspection = {
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

test.describe("match resolve transitions to mapping grid", () => {
  test.beforeEach(async ({ page }) => {
    // No mapping is seeded here — this fixture only exercises the resolver's
    // transition into the grid, which builds its own mapping from the
    // inspection's bucket_preview.
    await seed(page, { __matcherInspection__: inspection });
    await seedChapters(page, PROJECT_KEY, chapters(CHAPTER_COUNT, 0));
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

    // 85 chapter rows, 6 bands (one per contiguous track group).
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(
      CHAPTER_COUNT,
    );
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(
      TRACK_COUNT,
    );
  });
});
