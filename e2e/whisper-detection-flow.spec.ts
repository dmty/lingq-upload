import { type Page } from "@playwright/test";

import { expect, seed, test } from "./setup/test";
import { seedChapters } from "./setup/mapping-fixture";
import type { DetectionAvailabilitySeed } from "./setup/window";
import type {
  AppError,
  Chapter,
  DetectStartResult,
  DetectionPreview,
  MismatchInspection,
} from "../src/lib/ipc/bindings";

const PROJECT_KEY = "detection-flow-fixture";
const OTHER_PROJECT_KEY = "detection-flow-other";
const CANDIDATE_PROJECT_KEY = "detection-flow-candidates";
const START_ID = "spine:arrival";
const END_ID = "spine:return";

const PROJECT_TITLES: Record<string, string> = {
  [PROJECT_KEY]: "Range Detection Fixture",
  [OTHER_PROJECT_KEY]: "Other Detection Fixture",
  [CANDIDATE_PROJECT_KEY]: "Candidate Detection Fixture",
};

const chapters: Chapter[] = [
  {
    id: "spine:preface",
    order: 0,
    title: "Preface",
    body: "",
    kind: "front_matter",
  },
  { id: START_ID, order: 1, title: "Arrival", body: "", kind: "body" },
  { id: END_ID, order: 2, title: "Return", body: "", kind: "body" },
  {
    id: "spine:notes",
    order: 3,
    title: "Notes",
    body: "",
    kind: "back_matter",
  },
];

const candidateChapters: Chapter[] = [
  { id: "idx:0", order: 0, title: "Foreword", body: "", kind: "front_matter" },
  { id: "idx:1", order: 1, title: "Chapter One", body: "", kind: "body" },
  { id: "idx:2", order: 2, title: "The Crossing", body: "", kind: "body" },
  { id: "idx:3", order: 3, title: "Midpoint", body: "", kind: "body" },
  { id: "idx:4", order: 4, title: "The Descent", body: "", kind: "body" },
  { id: "idx:5", order: 5, title: "Epilogue", body: "", kind: "body" },
];

const PROJECT_KEYS = Object.keys(PROJECT_TITLES);

function chaptersFor(key: string): Chapter[] {
  return key === CANDIDATE_PROJECT_KEY ? candidateChapters : chapters;
}

const lowConfidence: DetectStartResult = {
  kind: "low_confidence",
  transcript_head_preview: "a crossing begins",
  transcript_tail_preview: "the descent ends",
  top_head: [
    { chapter_id: "idx:2", order: 2, title: "The Crossing", score: 0.62 },
    { chapter_id: "idx:4", order: 4, title: "The Descent", score: 0.55 },
  ],
  top_tail: [
    { chapter_id: "idx:4", order: 4, title: "The Descent", score: 0.58 },
    { chapter_id: "idx:3", order: 3, title: "Midpoint", score: 0.51 },
  ],
};

// title and chapter_count are always overridden per project below.
const inspectionTemplate: Omit<MismatchInspection, "title" | "chapter_count"> =
  {
    track_count: 2,
    condition: "many_to_few",
    options: ["split_proportional", "single_lesson", "cancel"],
    preselect: "split_proportional",
    bucket_preview: null,
  };

const titlePreview: DetectionPreview = {
  provider_id: null,
  align_source: "title",
  range: { start_chapter_id: START_ID, end_chapter_id: END_ID },
  confidence: 0.93,
  transcript_head_preview: null,
  transcript_tail_preview: null,
  detected_at: "2026-08-12T00:00:00Z",
  atom_starts: [],
};

const TRANSCRIPT_HEAD_PREVIEW = "始まり 🐉 café — arrival";
const TRANSCRIPT_TAIL_PREVIEW = "帰還 🌊 fin — return";

const transcriptPreview: DetectionPreview = {
  provider_id: "groq",
  align_source: "transcript",
  range: { start_chapter_id: START_ID, end_chapter_id: END_ID },
  confidence: 0.8123,
  transcript_head_preview: TRANSCRIPT_HEAD_PREVIEW,
  transcript_tail_preview: TRANSCRIPT_TAIL_PREVIEW,
  detected_at: "2026-08-12T00:01:00Z",
  atom_starts: [],
};

function fixture(): Partial<Window> {
  const inspections: Record<string, MismatchInspection> = {};
  const consents: Record<string, string> = {};
  const availabilities: Record<string, DetectionAvailabilitySeed> = {};
  for (const key of PROJECT_KEYS) {
    const projectChapters = chaptersFor(key);
    inspections[key] = {
      ...inspectionTemplate,
      title: PROJECT_TITLES[key],
      chapter_count: projectChapters.length,
    };
    consents[key] = "groq";
    availabilities[key] = {
      eligible: true,
      condition: "many_to_few",
      chapter_count: projectChapters.length,
      track_count: 2,
    };
  }
  return {
    __matcherInspectionByProject__: inspections,
    __transcriptionConsents__: consents,
    __detectionAvailabilityByProject__: availabilities,
    __transcriptionKeys__: { groq: true },
  };
}

async function seedFixture(page: Page): Promise<void> {
  await seed(page, fixture());
  for (const key of PROJECT_KEYS) {
    await seedChapters(page, key, chaptersFor(key));
  }
}

async function holdDetection(page: Page): Promise<void> {
  await page.evaluate(() => {
    window.__detectionGate__ = new Promise((resolve) => {
      window.__releaseDetection__ = resolve;
    });
  });
}

function startArgsLength(page: Page): Promise<number> {
  return page.evaluate(() => window.__detectionStartArgs__?.length ?? 0);
}

async function nthStartJobId(page: Page, index: number): Promise<string> {
  const jobId = await page.evaluate(
    (i) => window.__detectionStartArgs__?.[i]?.jobId,
    index,
  );
  if (!jobId) throw new Error(`no detection start recorded at index ${index}`);
  return jobId;
}

async function startDetection(
  page: Page,
  projectKey = PROJECT_KEY,
): Promise<string> {
  await page.goto(`/match/${projectKey}`);
  await expect(page.getByText(PROJECT_TITLES[projectKey])).toBeVisible();
  await holdDetection(page);
  await page.getByRole("button", { name: "Detect audio's text range" }).click();
  await expect.poll(() => startArgsLength(page)).toBe(1);
  return nthStartJobId(page, 0);
}

async function emit(page: Page, payload: object): Promise<void> {
  await page.evaluate((event) => window.__emitEvent__("job", event), payload);
}

async function emitDetected(
  page: Page,
  jobId: string,
  preview: DetectionPreview,
): Promise<void> {
  await emit(page, {
    kind: "Started",
    job_id: jobId,
    stage: { kind: "detecting_start" },
  });
  await emit(page, {
    kind: "Result",
    job_id: jobId,
    ok: true,
    payload: { kind: "detected", preview },
  });
}

async function confirmedRangeArgs(page: Page) {
  const args = await page.evaluate(
    () => window.__confirmDetectedRangeCalls__?.[0],
  );
  if (!args) throw new Error("cmd_confirm_detected_range was not called");
  return args;
}

/** Return a typed detection outcome straight from the command, no events. */
async function detectReturning(
  page: Page,
  projectKey: string,
  outcome: { result?: DetectStartResult; error?: AppError | Error },
): Promise<void> {
  await page.goto(`/match/${projectKey}`);
  await expect(page.getByText(PROJECT_TITLES[projectKey])).toBeVisible();
  await page.evaluate((next) => {
    if (next.result) window.__detectionResult__ = next.result;
    if (next.error) window.__detectionCommandError__ = next.error;
  }, outcome);
  await page.getByRole("button", { name: "Detect audio's text range" }).click();
}

function candidateRadio(page: Page, group: "Start chapter" | "End chapter") {
  return (name: RegExp) =>
    page.getByRole("group", { name: group }).getByRole("radio", { name });
}

test.describe("detected text range flow", () => {
  test.beforeEach(async ({ page }) => {
    await seedFixture(page);
  });

  test("renders scoped monotonic typed progress and cancels by caller job ID", async ({
    page,
  }) => {
    const jobId = await startDetection(page);
    expect(
      await page.evaluate(() => window.__detectionListenerCountAtStart__),
    ).toBeGreaterThan(0);
    expect(jobId).toMatch(/^[0-9a-f-]{36}$/);

    await emit(page, {
      kind: "DetectionProgress",
      job_id: "00000000-0000-4000-8000-000000000000",
      pct: 0.99,
      phase: "align_tail",
    });
    await expect(page.getByRole("progressbar")).toHaveAttribute("value", "0");

    await emit(page, {
      kind: "Started",
      job_id: jobId,
      stage: { kind: "detecting_start" },
    });
    const phases = [
      ["title_check", "Checking embedded titles"],
      ["sample_head", "Preparing start sample"],
      ["transcribe_head", "Transcribing start sample"],
      ["align_head", "Matching start chapter"],
      ["sample_tail", "Preparing end sample"],
      ["transcribe_tail", "Transcribing end sample"],
      ["align_tail", "Matching end chapter"],
    ] as const;
    for (const [index, [phase, label]] of phases.entries()) {
      await emit(page, {
        kind: "DetectionProgress",
        job_id: jobId,
        pct: (index + 1) / 10,
        phase,
      });
      await expect(page.getByRole("status")).toContainText(label);
    }
    await emit(page, {
      kind: "DetectionProgress",
      job_id: jobId,
      pct: 0.2,
      phase: "align_tail",
    });
    await expect(page.getByRole("progressbar")).toHaveAttribute("value", "0.7");

    await page.getByRole("button", { name: "Cancel detection" }).click();
    await expect
      .poll(() => page.evaluate(() => window.__cancelJobCalls__?.[0]?.jobId))
      .toBe(jobId);
    await emit(page, { kind: "Cancelled", job_id: jobId });
    await expect(page.getByRole("status")).toContainText("Detection cancelled");
    await expect(
      page.getByRole("button", { name: "Cancel detection" }),
    ).toHaveCount(0);
  });

  test("renders title and Unicode transcript quotes while Refine stays local", async ({
    page,
  }) => {
    let jobId = await startDetection(page);
    await emitDetected(page, jobId, titlePreview);

    const preview = page.getByTestId("detection-range-preview");
    await expect(preview).toContainText("Chapter 2 · Arrival");
    await expect(preview).toContainText("Chapter 3 · Return");
    await expect(preview).toContainText("Embedded titles");
    await expect(preview).toContainText("93%");
    await expect(preview.locator("details")).toHaveCount(0);

    await page.getByRole("button", { name: "Refine" }).click();
    await expect(preview).toHaveCount(0);
    await expect(page.getByText("Range Detection Fixture")).toBeVisible();
    expect(
      await page.evaluate(
        () =>
          window.__invokeLog__.filter(
            (command) => command === "cmd_reset_detection",
          ).length,
      ),
    ).toBe(0);

    await holdDetection(page);
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();
    await expect.poll(() => startArgsLength(page)).toBe(2);
    jobId = await nthStartJobId(page, 1);
    await emitDetected(page, jobId, transcriptPreview);

    await expect(preview).toContainText("Transcription");
    await expect(preview).toContainText("81%");
    await expect(preview.locator("details")).toHaveCount(0);
    await expect(preview.getByText(TRANSCRIPT_HEAD_PREVIEW)).toBeVisible();
    await expect(preview.getByText(TRANSCRIPT_TAIL_PREVIEW)).toBeVisible();
  });

  test("keeps content outcomes distinct from one typed operational error", async ({
    page,
  }) => {
    let jobId = await startDetection(page);
    await emit(page, {
      kind: "Started",
      job_id: jobId,
      stage: { kind: "detecting_start" },
    });
    await emit(page, {
      kind: "Result",
      job_id: jobId,
      ok: true,
      payload: {
        kind: "low_confidence",
        transcript_head_preview: "possible start",
        transcript_tail_preview: null,
        top_head: [
          { chapter_id: START_ID, order: 1, title: "Arrival", score: 0.51 },
        ],
        top_tail: [
          { chapter_id: END_ID, order: 2, title: "Return", score: 0.49 },
        ],
      },
    });
    await expect(page.getByText("Detection needs refinement")).toBeVisible();
    await expect(
      page.getByRole("group", { name: "Start chapter" }),
    ).toContainText("Arrival");
    await expect(page.getByRole("alert")).toHaveCount(0);

    await page.getByRole("button", { name: "Refine" }).click();
    await holdDetection(page);
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();
    await expect.poll(() => startArgsLength(page)).toBe(2);
    jobId = await nthStartJobId(page, 1);
    await page.evaluate(() => {
      window.__detectionCommandError__ = {
        kind: "Transcribe",
        message: { kind: "rate_limit", message: "rate limited" },
      };
    });
    await emit(page, {
      kind: "Started",
      job_id: jobId,
      stage: { kind: "detecting_start" },
    });
    await emit(page, {
      kind: "Result",
      job_id: jobId,
      ok: false,
      payload: { kind: "Transcribe", message: { kind: "network" } },
    });
    await page.evaluate(() => window.__releaseDetection__?.());
    await expect(page.getByRole("alert")).toHaveCount(1);
    await expect(page.getByRole("alert")).toContainText(
      "Couldn't reach the transcription provider",
    );
  });

  test("renders a command error that never emits a terminal event", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByText("Range Detection Fixture")).toBeVisible();
    await page.evaluate(() => {
      window.__detectionCommandError__ = {
        kind: "Transcribe",
        message: { kind: "api_key", message: "no key configured" },
      };
    });
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();

    await expect(page.getByRole("alert")).toHaveCount(1);
    await expect(page.getByRole("alert")).toContainText(
      "No transcription API key configured",
    );
    await expect(
      page.getByRole("button", { name: "Cancel detection" }),
    ).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Refine" })).toBeVisible();
  });

  test("renders a transport rejection instead of spinning forever", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByText("Range Detection Fixture")).toBeVisible();
    await page.evaluate(() => {
      window.__detectionCommandError__ = new Error("ipc transport unavailable");
    });
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();

    await expect(page.getByRole("alert")).toContainText(
      "ipc transport unavailable",
    );
    await expect(
      page.getByRole("button", { name: "Cancel detection" }),
    ).toHaveCount(0);
  });

  test("renders a cached preview returned without any events", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByText("Range Detection Fixture")).toBeVisible();
    await page.evaluate((preview) => {
      window.__detectionResult__ = { kind: "detected", preview };
    }, titlePreview);
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();

    const preview = page.getByTestId("detection-range-preview");
    await expect(preview).toContainText("Chapter 2 · Arrival");
    await expect(preview).toContainText("Chapter 3 · Return");
    await expect(
      page.getByRole("button", { name: "Cancel detection" }),
    ).toHaveCount(0);
  });

  test("detected atom starts list each audio part", async ({ page }) => {
    const jobId = await startDetection(page);
    await emitDetected(page, jobId, {
      ...transcriptPreview,
      atom_starts: [
        { track_index: 0, chapter_id: START_ID },
        { track_index: 1, chapter_id: END_ID },
      ],
    });

    const starts = page.getByTestId("detection-atom-starts");
    await expect(starts).toContainText("Part 1");
    await expect(starts).toContainText("Chapter 2 · Arrival");
    await expect(starts).toContainText("Part 2");
    await expect(starts).toContainText("Chapter 3 · Return");
    await expect(page.getByTestId("detection-range-preview")).toContainText(
      "Heard each audio part",
    );
  });

  test("confirms stable IDs into MappingGrid and routes only from mapping confirmation", async ({
    page,
  }) => {
    const jobId = await startDetection(page);
    await emitDetected(page, jobId, transcriptPreview);

    await page.getByRole("button", { name: "Confirm detected range" }).click();
    await expect(page).toHaveURL(new RegExp(`/match/${PROJECT_KEY}$`));
    await expect(page.getByTestId("mapping-grid")).toBeVisible();

    const args = await confirmedRangeArgs(page);
    expect(args.selectedRange).toEqual({
      start_chapter_id: START_ID,
      end_chapter_id: END_ID,
    });
    expect(args.evidence.range).toEqual(args.selectedRange);
    expect(args.evidence.confidence).toBe(transcriptPreview.confidence);

    await page.getByTestId("mapping-continue").click();
    await expect(page).toHaveURL(new RegExp(`/run/${PROJECT_KEY}`));
  });

  test("low-confidence candidates expose ordinals, scores, and keyboard choice", async ({
    page,
  }) => {
    await detectReturning(page, CANDIDATE_PROJECT_KEY, {
      result: lowConfidence,
    });
    const startRadio = candidateRadio(page, "Start chapter");
    const endRadio = candidateRadio(page, "End chapter");

    await expect(
      startRadio(/Chapter 3 · The Crossing · 62% match/),
    ).toBeChecked();
    await expect(
      startRadio(/Chapter 5 · The Descent · 55% match/),
    ).not.toBeChecked();
    await expect(endRadio(/Chapter 5 · The Descent · 58% match/)).toBeChecked();
    await expect(
      endRadio(/Chapter 4 · Midpoint · 51% match/),
    ).not.toBeChecked();
    await expect(startRadio(/Epilogue/)).toHaveCount(0);

    const fallback = endRadio(/Chapter 6 · Epilogue.*final chapter fallback/i);
    await expect(fallback).not.toBeChecked();

    const preview = page.getByTestId("detection-range-preview");
    await expect(preview).toContainText("Transcription");
    await expect(preview).toContainText("60%");

    await startRadio(/Chapter 3 · The Crossing/).focus();
    await page.keyboard.press("ArrowDown");
    await expect(
      startRadio(/Chapter 5 · The Descent · 55% match/),
    ).toBeChecked();
    await page.keyboard.press("ArrowUp");
    await expect(
      startRadio(/Chapter 3 · The Crossing · 62% match/),
    ).toBeChecked();

    await fallback.focus();
    await page.keyboard.press("Space");
    await expect(fallback).toBeChecked();
    await expect(preview).toContainText("62%");

    await page.getByRole("button", { name: "Confirm detected range" }).click();
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    const args = await confirmedRangeArgs(page);
    expect(args.selectedRange).toEqual({
      start_chapter_id: "idx:2",
      end_chapter_id: "idx:5",
    });
    expect(args.evidence.range).toEqual(args.selectedRange);
    expect(args.evidence.align_source).toBe("transcript");
    expect(args.evidence.provider_id).toBe("groq");
    expect(args.evidence.confidence).toBeCloseTo(0.62, 5);
    expect(args.evidence.transcript_head_preview).toBe("a crossing begins");
    expect(args.evidence.detected_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  test("the final-chapter fallback offers the last eligible chapter", async ({
    page,
  }) => {
    await page.addInitScript(
      (skipped: Record<string, string[]>) =>
        window.__pickerState__._writeSkipped(skipped),
      { [CANDIDATE_PROJECT_KEY]: ["idx:5"] },
    );
    await detectReturning(page, CANDIDATE_PROJECT_KEY, {
      result: {
        ...lowConfidence,
        top_tail: [
          { chapter_id: "idx:3", order: 3, title: "Midpoint", score: 0.58 },
        ],
      },
    });
    const endRadio = candidateRadio(page, "End chapter");
    await expect(endRadio(/Epilogue/)).toHaveCount(0);

    const fallback = endRadio(
      /Chapter 5 · The Descent.*final chapter fallback/i,
    );
    await expect(fallback).not.toBeChecked();
    await fallback.check();

    await page.getByRole("button", { name: "Confirm detected range" }).click();
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    const args = await confirmedRangeArgs(page);
    expect(args.selectedRange).toEqual({
      start_chapter_id: "idx:2",
      end_chapter_id: "idx:4",
    });
  });

  test("an end-before-start candidate pair cannot be confirmed", async ({
    page,
  }) => {
    await detectReturning(page, CANDIDATE_PROJECT_KEY, {
      result: lowConfidence,
    });
    await candidateRadio(
      page,
      "Start chapter",
    )(/Chapter 5 · The Descent/).check();
    await candidateRadio(page, "End chapter")(/Chapter 4 · Midpoint/).check();

    const confirm = page.getByRole("button", {
      name: "Confirm detected range",
    });
    await expect(confirm).toBeDisabled();
    await expect(page.getByTestId("detection-range-validation")).toContainText(
      "The end chapter comes before the start chapter",
    );

    await candidateRadio(page, "End chapter")(/Chapter 6 · Epilogue/).check();
    await expect(confirm).toBeEnabled();
    await expect(page.getByTestId("detection-range-validation")).toHaveCount(0);
  });

  test("title collision leaves the end unset and offers a whole-book split", async ({
    page,
  }) => {
    await detectReturning(page, CANDIDATE_PROJECT_KEY, {
      result: {
        kind: "low_confidence",
        transcript_head_preview: "時をかける少女",
        transcript_tail_preview: "時をかける少女",
        top_head: [
          { chapter_id: "idx:2", order: 2, title: "The Crossing", score: 1 },
          { chapter_id: "idx:1", order: 1, title: "Chapter One", score: 0.2 },
        ],
        top_tail: [
          { chapter_id: "idx:2", order: 2, title: "The Crossing", score: 1 },
          { chapter_id: "idx:5", order: 5, title: "Epilogue", score: 0.13 },
        ],
      },
    });

    const assist = page.getByTestId("detection-cue-sheet");
    await expect(assist).toContainText("opening of each audio part");
    await expect(assist).toContainText("時をかける少女");
    await expect(page.getByTestId("detection-title-collision")).toBeVisible();
    await expect(page.getByTestId("detection-range-preview")).toHaveCount(0);

    await page.getByText("Or trim title page / credits first").click();
    await expect(
      candidateRadio(page, "Start chapter")(/Chapter 3 · The Crossing/),
    ).toBeChecked();
    await expect(
      candidateRadio(page, "End chapter")(/Chapter 3 · The Crossing/),
    ).not.toBeChecked();

    await page.getByTestId("detection-use-whole-book").click();
    await expect(
      page.getByTestId("mismatch-response-split_proportional"),
    ).toHaveClass(/ring-accent/);
  });

  test("a single-chapter range cannot be confirmed when other chapters exist", async ({
    page,
  }) => {
    await detectReturning(page, CANDIDATE_PROJECT_KEY, {
      result: lowConfidence,
    });
    await candidateRadio(
      page,
      "Start chapter",
    )(/Chapter 5 · The Descent/).check();
    await candidateRadio(
      page,
      "End chapter",
    )(/Chapter 5 · The Descent/).check();

    const confirm = page.getByRole("button", {
      name: "Confirm detected range",
    });
    await expect(confirm).toBeDisabled();
    await expect(page.getByTestId("detection-range-validation")).toContainText(
      "would drop the rest of the book",
    );
  });

  test("stale candidate IDs are refused instead of silently dropped", async ({
    page,
  }) => {
    await detectReturning(page, CANDIDATE_PROJECT_KEY, {
      result: {
        ...lowConfidence,
        top_head: [
          { chapter_id: "idx:99", order: 9, title: "Removed", score: 0.7 },
        ],
      },
    });
    const assist = page.getByTestId("detection-assist");
    await expect(assist).toContainText(
      "These suggestions no longer match this book",
    );
    await expect(
      page.getByRole("group", { name: "Start chapter" }),
    ).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Confirm detected range" }),
    ).toHaveCount(0);
    await expect(
      assist.getByRole("button", { name: "Try detection again" }),
    ).toBeVisible();
  });

  const contentOutcomes = [
    { reason: "empty", copy: "No speech was recognized", retry: true },
    {
      reason: "content_poor",
      copy: "The sample did not contain enough distinctive words",
      retry: true,
    },
    {
      reason: "insufficient_audio",
      copy: "The resolved audio range is too short for two safe samples.",
      retry: false,
    },
  ] as const;

  for (const outcome of contentOutcomes) {
    test(`content outcome ${outcome.reason} explains the limit without blaming the provider`, async ({
      page,
    }) => {
      await detectReturning(page, PROJECT_KEY, {
        result: { kind: "no_transcript", reason: outcome.reason },
      });
      const assist = page.getByTestId("detection-assist");
      await expect(page.getByTestId("detection-content-outcome")).toContainText(
        outcome.copy,
      );
      await expect(assist.getByText(/provider (failed|rejected)/i)).toHaveCount(
        0,
      );
      await expect(page.getByRole("alert")).toHaveCount(0);
      await expect(
        assist.getByRole("button", { name: "Try detection again" }),
      ).toHaveCount(outcome.retry ? 1 : 0);
      await expect(
        assist.getByRole("button", { name: "Refine" }),
      ).toBeVisible();
      await expect(
        page.getByRole("button", { name: "Split by embedded chapters" }),
      ).toBeVisible();
    });
  }

  const operationalErrors = [
    {
      kind: "api_key",
      copy: "No transcription API key configured",
      link: "Open transcription settings",
      retry: false,
    },
    {
      kind: "unauthorized",
      copy: "rejected the API key",
      link: "Open transcription settings",
      retry: false,
    },
    {
      kind: "rate_limit",
      copy: "rate limit was reached",
      link: "Switch transcription provider",
      retry: true,
    },
    {
      kind: "provider_failed",
      copy: "The transcription provider failed",
      link: "Switch transcription provider",
      retry: true,
    },
    {
      kind: "timeout",
      copy: "Transcription timed out",
      link: null,
      retry: true,
    },
    {
      kind: "network",
      copy: "Check your internet connection",
      link: null,
      retry: true,
    },
    {
      kind: "audio",
      copy: "Couldn't prepare this audio for transcription",
      link: null,
      retry: false,
    },
  ] as const;

  for (const failure of operationalErrors) {
    test(`operational error ${failure.kind} offers typed recovery actions`, async ({
      page,
    }) => {
      await detectReturning(page, PROJECT_KEY, {
        error: {
          kind: "Transcribe",
          message: { kind: failure.kind, message: failure.copy },
        },
      });
      const assist = page.getByTestId("detection-assist");
      await expect(page.getByRole("alert")).toContainText(failure.copy);
      if (failure.link) {
        await expect(
          assist.getByRole("link", { name: failure.link }),
        ).toBeVisible();
      } else {
        await expect(assist.getByRole("link")).toHaveCount(0);
      }
      await expect(
        assist.getByRole("button", { name: "Try detection again" }),
      ).toHaveCount(failure.retry ? 1 : 0);
      if (failure.kind === "audio") {
        await expect(assist).toContainText("choose a manual response");
      }
      await expect(
        assist.getByRole("button", { name: "Refine" }),
      ).toBeVisible();
      await expect(
        page.getByRole("button", { name: "Split by embedded chapters" }),
      ).toBeVisible();
    });
  }

  test("operational error retry restarts detection only when asked", async ({
    page,
  }) => {
    await detectReturning(page, PROJECT_KEY, {
      error: {
        kind: "Transcribe",
        message: { kind: "network", message: "Check your internet connection" },
      },
    });
    const assist = page.getByTestId("detection-assist");
    await expect(page.getByRole("alert")).toBeVisible();
    expect(await startArgsLength(page)).toBe(1);

    await page.evaluate(() => {
      window.__detectionCommandError__ = undefined;
      window.__detectionResult__ = { kind: "no_transcript", reason: "empty" };
    });
    await assist.getByRole("button", { name: "Try detection again" }).click();
    await expect.poll(() => startArgsLength(page)).toBe(2);
    await expect(page.getByTestId("detection-content-outcome")).toContainText(
      "No speech was recognized",
    );
    await expect(page.getByRole("alert")).toHaveCount(0);
  });

  test("an old detection cannot update a newly navigated project", async ({
    page,
  }) => {
    const oldJobId = await startDetection(page);
    await page.goto(`/match/${OTHER_PROJECT_KEY}`);
    await expect(page.getByText("Other Detection Fixture")).toBeVisible();

    await emitDetected(page, oldJobId, transcriptPreview);
    await expect(page.getByTestId("detection-range-preview")).toHaveCount(0);
    await expect(page.getByRole("alert")).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Detect audio's text range" }),
    ).toBeVisible();
  });
});
