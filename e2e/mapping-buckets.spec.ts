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
  // Set a non-null inspection so hydrateFromBackend doesn't redirect to /run.
  // The mapping is already seeded so the grid renders; inspection just satisfies
  // the page's "no pending decision" guard.
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

const DRIFT_KEY = "drift-fixture";

function driftFixtureScript(): string {
  // 3 tracks: t0 and t1 have charsPerSec 5 (median = 5), t2 has charsPerSec 12 (~140% deviation → drift).
  const chapters = Array.from({ length: 3 }, (_, i) => ({
    id: `dr:${i}`,
    order: i,
    title: `Chapter ${i + 1}`,
    body: "x".repeat(100),
    kind: "body",
  }));
  const mapping = {
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
  const inspection = {
    title: "Drift Fixture",
    chapter_count: 3,
    track_count: 3,
    condition: "many_to_few" as const,
    options: ["split_proportional", "cancel"] as const,
    preselect: "split_proportional" as const,
    bucket_preview: null,
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
    window.__pickerState__.chaptersByProject[${JSON.stringify(DRIFT_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = ${JSON.stringify(inspection)};
    window.__mappingState__.seed(${JSON.stringify(DRIFT_KEY)}, ${JSON.stringify(mapping)});
  })();`;
}

const NON_CONTIGUOUS_KEY = "nc-fixture";

function nonContiguousFixtureScript(): string {
  // 3 chapters: t0, t1, t0 — non-adjacent same track_id produces 3 distinct bands.
  const chapters = Array.from({ length: 3 }, (_, i) => ({
    id: `nc:${i}`,
    order: i,
    title: `Chapter ${i + 1}`,
    body: "x".repeat(100),
    kind: "body",
  }));
  const mapping = {
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
  const inspection = {
    title: "NC Fixture",
    chapter_count: 3,
    track_count: 2,
    condition: "many_to_few" as const,
    options: ["split_proportional", "cancel"] as const,
    preselect: "split_proportional" as const,
    bucket_preview: null,
  };
  return `;(() => {
    window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
    window.__pickerState__.chaptersByProject[${JSON.stringify(NON_CONTIGUOUS_KEY)}] = ${JSON.stringify(chapters)};
    window.__matcherInspection__ = ${JSON.stringify(inspection)};
    window.__mappingState__.seed(${JSON.stringify(NON_CONTIGUOUS_KEY)}, ${JSON.stringify(mapping)});
  })();`;
}

test.describe("banded bucket list", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
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
    await page.addInitScript(driftFixtureScript());
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
    await page.addInitScript(nonContiguousFixtureScript());
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
    const chapters = [
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
    const mapping = {
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
    const inspection = {
      title: "Orphan Fixture",
      chapter_count: 2,
      track_count: 2,
      condition: "many_to_few" as const,
      options: ["split_proportional", "cancel"] as const,
      preselect: "split_proportional" as const,
      bucket_preview: null,
    };
    await page.addInitScript(`;(() => {
      window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
      window.__pickerState__.chaptersByProject[${JSON.stringify(ORPHAN_KEY)}] = ${JSON.stringify(chapters)};
      window.__matcherInspection__ = ${JSON.stringify(inspection)};
      window.__mappingState__.seed(${JSON.stringify(ORPHAN_KEY)}, ${JSON.stringify(mapping)});
    })();`);
    await page.goto(`/match/${ORPHAN_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    // 3 bands: t0 (paired), t1 (orphan, audio-only), tail (unpaired chapter).
    await expect(page.getByTestId("mapping-bucket-band")).toHaveCount(3);
    // Orphan band still shows audio metadata.
    await expect(page.getByTestId("bucket-band-meta")).toHaveCount(2);
    await expect(page.getByTestId("bucket-band-meta").nth(1)).toContainText(
      "Audio 2",
    );
    // Tail-band chapter (Chapter 2) gets a ↑ arrow targeting the orphan t1;
    // paired Chapter 1 gets a ↓ arrow targeting the orphan as well.
    await expect(page.getByTestId("chapter-move-up")).toHaveCount(1);
    await expect(page.getByTestId("chapter-move-up")).toHaveAttribute(
      "data-chapter-id",
      "or:1",
    );
    await expect(page.getByTestId("chapter-move-down")).toHaveCount(1);
    await expect(page.getByTestId("chapter-move-down")).toHaveAttribute(
      "data-chapter-id",
      "or:0",
    );
  });

  test("orphan bucket stays in audio-order position between paired buckets", async ({
    page,
  }) => {
    // Buckets [t0, t1, t2]. Only t0 and t2 carry a chapter. t1 is orphan and
    // must render between them, not appended at the end.
    const MID_KEY = "orphan-mid-fixture";
    const chapters = [
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
    const mapping = {
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
    const inspection = {
      title: "Orphan Mid Fixture",
      chapter_count: 2,
      track_count: 3,
      condition: "many_to_few" as const,
      options: ["split_proportional", "cancel"] as const,
      preselect: "split_proportional" as const,
      bucket_preview: null,
    };
    await page.addInitScript(`;(() => {
      window.__pickerState__ = window.__pickerState__ || { skippedByProject: {}, chaptersByProject: {} };
      window.__pickerState__.chaptersByProject[${JSON.stringify(MID_KEY)}] = ${JSON.stringify(chapters)};
      window.__matcherInspection__ = ${JSON.stringify(inspection)};
      window.__mappingState__.seed(${JSON.stringify(MID_KEY)}, ${JSON.stringify(mapping)});
    })();`);
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
