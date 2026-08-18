import type {
  Chapter,
  MappingState,
  MismatchInspection,
} from "../src/lib/ipc/bindings";
import { expect, test } from "./setup/test";
import { installMapping, pair } from "./setup/mapping-fixture";

const PROJECT_KEY = "bucket-fixture";

const mapping: MappingState = {
  pairs: [
    pair(0, "t0", 1),
    pair(1, "t0", 1),
    pair(2, "t0", 1),
    pair(3, "t1", 1),
    pair(4, "t1", 1),
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

// Set a non-null inspection so hydrateFromBackend doesn't redirect to /run.
// The mapping is already seeded so the grid renders; inspection just satisfies
// the page's "no pending decision" guard.
const inspection: MismatchInspection = {
  title: "Bucket Fixture",
  chapter_count: 5,
  track_count: 2,
  condition: "many_to_few",
  options: ["split_proportional", "cancel"],
  preselect: "split_proportional",
  bucket_preview: null,
};

const DRIFT_KEY = "drift-fixture";

// 3 tracks: t0 and t1 have charsPerSec 5 (median = 5), t2 has charsPerSec 12
// (~140% deviation → drift).
const driftChapters: Chapter[] = Array.from({ length: 3 }, (_, i) => ({
  id: `dr:${i}`,
  order: i,
  title: `Chapter ${i + 1}`,
  body: "x".repeat(100),
  kind: "body",
}));

const driftMapping: MappingState = {
  pairs: [
    {
      chapter_id: "dr:0",
      track_id: "t0",
      confidence: 1,
      touched: false,
      original_confidence: 1,
    },
    {
      chapter_id: "dr:1",
      track_id: "t1",
      confidence: 1,
      touched: false,
      original_confidence: 1,
    },
    {
      chapter_id: "dr:2",
      track_id: "t2",
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
      atomDurationSec: 300,
      charsPerSec: 5,
      audioPath: "/audio/t0.m4a",
      window: [0, 300],
    },
    {
      trackId: "t1",
      atomTitle: "Audio 2",
      atomDurationSec: 300,
      charsPerSec: 5,
      audioPath: "/audio/t1.m4a",
      window: [0, 300],
    },
    {
      trackId: "t2",
      atomTitle: "Audio 3",
      atomDurationSec: 300,
      charsPerSec: 12,
      audioPath: "/audio/t2.m4a",
      window: [0, 300],
    },
  ],
};

const driftInspection: MismatchInspection = {
  title: "Drift Fixture",
  chapter_count: 3,
  track_count: 3,
  condition: "many_to_few",
  options: ["split_proportional", "cancel"],
  preselect: "split_proportional",
  bucket_preview: null,
};

const NON_CONTIGUOUS_KEY = "nc-fixture";

// 3 chapters: t0, t1, t0 — non-adjacent same track_id produces 3 distinct bands.
const nonContiguousChapters: Chapter[] = Array.from({ length: 3 }, (_, i) => ({
  id: `nc:${i}`,
  order: i,
  title: `Chapter ${i + 1}`,
  body: "x".repeat(100),
  kind: "body",
}));

const nonContiguousMapping: MappingState = {
  pairs: [
    {
      chapter_id: "nc:0",
      track_id: "t0",
      confidence: 1,
      touched: false,
      original_confidence: 1,
    },
    {
      chapter_id: "nc:1",
      track_id: "t1",
      confidence: 1,
      touched: false,
      original_confidence: 1,
    },
    {
      chapter_id: "nc:2",
      track_id: "t0",
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

const nonContiguousInspection: MismatchInspection = {
  title: "NC Fixture",
  chapter_count: 3,
  track_count: 2,
  condition: "many_to_few",
  options: ["split_proportional", "cancel"],
  preselect: "split_proportional",
  bucket_preview: null,
};

test.describe("banded bucket list", () => {
  test.beforeEach(async ({ page }) => {
    await installMapping(page, { key: PROJECT_KEY, mapping, inspection });
  });

  test("renders bands grouped by track with numbered chapters", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(2);
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(5);
    // chapters are numbered 1..5 in order
    await expect(page.getByTestId("chapter-number").first()).toHaveText("1");
    await expect(page.getByTestId("chapter-number").last()).toHaveText("5");
    // band header shows audio title + a formatted duration
    await expect(page.getByTestId("bucket-band-meta").first()).toContainText(
      "Audio 1",
    );
    await expect(page.getByTestId("bucket-band-meta").first()).toContainText(
      "10:00",
    );
    // no SVG connector layer
    await expect(
      page.locator('[data-testid="mapping-connector-layer"]'),
    ).toHaveCount(0);
  });

  test("flags a drifting band", async ({ page }) => {
    await installMapping(page, {
      key: DRIFT_KEY,
      chapters: driftChapters,
      mapping: driftMapping,
      inspection: driftInspection,
    });
    await page.goto(`/match/${DRIFT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    // Only t2 (charsPerSec 12) deviates >30% from median 5; t0 and t1 are at the median.
    await expect(page.getByTestId("bucket-drift")).toHaveCount(1);
  });

  test("renders one band per bucket in audio order; same-track chapters share a band", async ({
    page,
  }) => {
    // Synthetic non-contiguous case (c0→t0, c1→t1, c2→t0). The adjacency
    // invariant prevents this in production, but the renderer must still be
    // sane: one band per bucket in audio order, so t0's band holds both
    // chapters in EPUB order.
    await installMapping(page, {
      key: NON_CONTIGUOUS_KEY,
      chapters: nonContiguousChapters,
      mapping: nonContiguousMapping,
      inspection: nonContiguousInspection,
    });
    await page.goto(`/match/${NON_CONTIGUOUS_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(2);
    await expect(page.getByTestId("mapping-chapter-row")).toHaveCount(3);
    // EPUB numbering follows chapter order (c0=#1, c1=#2, c2=#3) but DOM order
    // groups by bucket: t0 (#1, #3), then t1 (#2). Document order is 1, 3, 2.
    const numbers = await page.getByTestId("chapter-number").allInnerTexts();
    expect(numbers).toEqual(["1", "3", "2"]);
  });

  test("orphan bucket renders as empty band; unpaired tail chapter can move up into it", async ({
    page,
  }) => {
    // 1 chapter paired to t0, 1 chapter unpaired (tail). Buckets list contains
    // BOTH t0 and t1 — t1 has no paired chapter (orphan). Expect bands:
    //   [t0 + Chapter 1], [t1 empty], [tail + Chapter 2].
    const ORPHAN_KEY = "orphan-fixture";
    const orphanChapters: Chapter[] = [
      {
        id: "or:0",
        order: 0,
        title: "Chapter 1",
        body: "x".repeat(100),
        kind: "body",
      },
      {
        id: "or:1",
        order: 1,
        title: "Chapter 2",
        body: "x".repeat(100),
        kind: "body",
      },
    ];
    const orphanMapping: MappingState = {
      pairs: [
        {
          chapter_id: "or:0",
          track_id: "t0",
          confidence: 1,
          touched: false,
          original_confidence: 1,
        },
        {
          chapter_id: "or:1",
          track_id: null,
          confidence: 0,
          touched: false,
          original_confidence: 0,
        },
      ],
      parking_lot: [],
      op_id: 0,
      buckets: [
        {
          trackId: "t0",
          atomTitle: "Audio 1",
          atomDurationSec: 300,
          charsPerSec: 5,
          audioPath: "/audio/t0.m4a",
          window: [0, 300],
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
    const orphanInspection: MismatchInspection = {
      title: "Orphan Fixture",
      chapter_count: 2,
      track_count: 2,
      condition: "many_to_few",
      options: ["split_proportional", "cancel"],
      preselect: "split_proportional",
      bucket_preview: null,
    };
    await installMapping(page, {
      key: ORPHAN_KEY,
      chapters: orphanChapters,
      mapping: orphanMapping,
      inspection: orphanInspection,
    });
    await page.goto(`/match/${ORPHAN_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    // 3 bands: t0 (paired), t1 (orphan, audio-only), tail (unpaired chapter).
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(3);
    // Orphan band still shows audio metadata.
    await expect(page.getByTestId("bucket-band-meta")).toHaveCount(2);
    await expect(page.getByTestId("bucket-band-meta").nth(1)).toContainText(
      "Audio 2",
    );
    // Arrows render on every row now, disabled where illegal. Tail-band
    // chapter (Chapter 2) gets an enabled ↑ targeting the orphan t1, and its
    // ↓ is disabled (last band). Paired Chapter 1's ↑ is disabled (first
    // band); its ↓ is enabled, targeting the orphan.
    await expect(page.getByTestId("chapter-move-up")).toHaveCount(2);
    await expect(page.getByTestId("chapter-move-up").nth(0)).toBeDisabled();
    await expect(page.getByTestId("chapter-move-up").nth(1)).toBeEnabled();
    await expect(page.getByTestId("chapter-move-up").nth(1)).toHaveAttribute(
      "data-chapter-id",
      "or:1",
    );
    await expect(page.getByTestId("chapter-move-down")).toHaveCount(2);
    await expect(page.getByTestId("chapter-move-down").nth(0)).toBeEnabled();
    await expect(page.getByTestId("chapter-move-down").nth(0)).toHaveAttribute(
      "data-chapter-id",
      "or:0",
    );
    await expect(page.getByTestId("chapter-move-down").nth(1)).toBeDisabled();
  });

  test("orphan bucket stays in audio-order position between paired buckets", async ({
    page,
  }) => {
    // Buckets [t0, t1, t2]. Only t0 and t2 carry a chapter. t1 is orphan and
    // must render between them, not appended at the end.
    const MID_KEY = "orphan-mid-fixture";
    const midChapters: Chapter[] = [
      {
        id: "om:0",
        order: 0,
        title: "Chapter 1",
        body: "x".repeat(100),
        kind: "body",
      },
      {
        id: "om:1",
        order: 1,
        title: "Chapter 2",
        body: "x".repeat(100),
        kind: "body",
      },
    ];
    const midMapping: MappingState = {
      pairs: [
        {
          chapter_id: "om:0",
          track_id: "t0",
          confidence: 1,
          touched: false,
          original_confidence: 1,
        },
        {
          chapter_id: "om:1",
          track_id: "t2",
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
          atomDurationSec: 300,
          charsPerSec: 5,
          audioPath: "/audio/t0.m4a",
          window: [0, 300],
        },
        {
          trackId: "t1",
          atomTitle: "Audio 2",
          atomDurationSec: 300,
          charsPerSec: 5,
          audioPath: "/audio/t1.m4a",
          window: [0, 300],
        },
        {
          trackId: "t2",
          atomTitle: "Audio 3",
          atomDurationSec: 300,
          charsPerSec: 5,
          audioPath: "/audio/t2.m4a",
          window: [0, 300],
        },
      ],
    };
    const midInspection: MismatchInspection = {
      title: "Orphan Mid Fixture",
      chapter_count: 2,
      track_count: 3,
      condition: "many_to_few",
      options: ["split_proportional", "cancel"],
      preselect: "split_proportional",
      bucket_preview: null,
    };
    await installMapping(page, {
      key: MID_KEY,
      chapters: midChapters,
      mapping: midMapping,
      inspection: midInspection,
    });
    await page.goto(`/match/${MID_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    // 3 bands, in audio order: Audio 1 (paired), Audio 2 (orphan), Audio 3 (paired).
    await expect(page.getByTestId("bucket-band-meta")).toHaveCount(3);
    await expect(page.getByTestId("bucket-band-meta").nth(0)).toContainText(
      "Audio 1",
    );
    await expect(page.getByTestId("bucket-band-meta").nth(1)).toContainText(
      "Audio 2",
    );
    await expect(page.getByTestId("bucket-band-meta").nth(2)).toContainText(
      "Audio 3",
    );
  });
});
