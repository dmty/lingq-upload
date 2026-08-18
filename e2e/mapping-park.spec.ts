import type { MappingState } from "../src/lib/ipc/bindings";
import { expect, test } from "./setup/test";
import { chapters, installMapping, pair } from "./setup/mapping-fixture";

const PROJECT_KEY = "park-fixture";

const mapping: MappingState = {
  pairs: [
    pair(0, "t0", 1),
    pair(1, "t0", 1),
    pair(2, "t0", 1),
    pair(3, "t1", 1),
    pair(4, "t2", 1),
  ],
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
    {
      trackId: "t2",
      atomTitle: "Audio 3",
      atomDurationSec: 300,
      charsPerSec: 5,
      audioPath: "/x/a2.m4b",
      window: null,
    },
  ],
};

test.describe("parking via button", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters: chapters(5),
      mapping,
      inspection: null,
    });
  });

  test("Park button parks the track and the lot shows its title", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(5);

    // Second band header = t1 ("Audio 2").
    await page.getByTestId("band-park").nth(1).click();

    await expect(page.getByTestId("parking-lot-count")).toHaveText("1");
    await expect(page.getByTestId("parked-track")).toContainText("Audio 2");
    await expect(page.getByTestId("parked-track")).not.toContainText("t1");
    // Chapter 4 is now unpaired, not lost.
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(5);

    // Restore round-trips.
    await page.getByTestId("parked-track-restore").click();
    await page
      .getByTestId("parked-track-chapters")
      .getByRole("button", { name: "Chapter 4" })
      .click();
    await expect(page.getByTestId("parking-lot-count")).toHaveText("0");
  });
});
