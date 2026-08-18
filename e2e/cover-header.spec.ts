import { expect, seed, test } from "./setup/test";
import { chapters, installMapping, pair } from "./setup/mapping-fixture";
import type { BucketMeta, MismatchInspection } from "../src/lib/ipc/bindings";

const PROJECT_KEY = "cover-fixture";

const projectChapters = chapters(4, 50);

const buckets: BucketMeta[] = [
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
];

const mapping = {
  pairs: projectChapters.map((c, i) => pair(i, i < 2 ? "t0" : "t1", 1)),
  parking_lot: [],
  op_id: 0,
  buckets,
};

const inspection: MismatchInspection = {
  title: "Botchan",
  chapter_count: 4,
  track_count: 2,
  condition: "many_to_few",
  options: ["split_proportional", "cancel"],
  preselect: "split_proportional",
  bucket_preview: null,
};

test.describe("match cover header", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters: projectChapters,
      mapping,
      inspection,
    });
    await seed(page, {
      __projectMeta__: {
        [PROJECT_KEY]: {
          title: "Botchan",
          authors: ["Natsume Soseki"],
          cover_path: null,
        },
      },
      __dialogPickPath__: "/picked/botchan-cover.png",
    });
  });

  test("shows cover, title, author, and an Add-cover control", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("match-title")).toHaveText("Botchan");
    await expect(page.getByTestId("match-author")).toHaveText("Natsume Soseki");
    await expect(page.getByTestId("match-cover")).toBeVisible();
    // cover_path is null → no real <img>, fallback tile, button reads "Add cover".
    await expect(page.getByTestId("cover-replace")).toHaveText("Add cover");
  });

  test("replace picks a file and flips the control to Replace cover", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("cover-replace")).toHaveText("Add cover");
    await page.getByTestId("cover-replace").click();
    // open() returns __dialogPickPath__ → cmdSetCover → coverPath state updates.
    await expect(page.getByTestId("cover-replace")).toHaveText("Replace cover");
  });
});
