import { expect, test, type Page } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";
import type { AppError, DetectStartResult } from "../src/lib/ipc/bindings";

const AUTO_KEY = "auto-eligible";
const EVIDENCE_KEY = "auto-existing-evidence";
const START_ID = "spine:arrival";
const END_ID = "spine:return";
const DETECTED_AT = "2026-08-12T09:30:00Z";

const chapters = [
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

const mapping = {
  pairs: [
    {
      chapter_id: START_ID,
      track_id: "t0",
      confidence: 1,
      original_confidence: 1,
      touched: false,
    },
    {
      chapter_id: END_ID,
      track_id: "t1",
      confidence: 1,
      original_confidence: 1,
      touched: false,
    },
  ],
  parking_lot: [],
  op_id: 0,
};

const detectedPreview = {
  provider_id: "groq",
  align_source: "transcript",
  range: { start_chapter_id: START_ID, end_chapter_id: END_ID },
  confidence: 0.8123,
  transcript_head_preview: "arrival",
  transcript_tail_preview: "return",
  detected_at: DETECTED_AT,
  atom_starts: [],
};

const lowConfidence: DetectStartResult = {
  kind: "low_confidence",
  transcript_head_preview: "possible arrival",
  transcript_tail_preview: "possible return",
  top_head: [{ chapter_id: START_ID, order: 1, title: "Arrival", score: 0.52 }],
  top_tail: [{ chapter_id: END_ID, order: 2, title: "Return", score: 0.49 }],
};

type Fixture = {
  key: string;
  title: string;
  availability: {
    eligible: boolean;
    condition: string | null;
    chapter_count: number;
    track_count: number;
    key_present?: boolean;
    existing_evidence?: object | null;
  };
  consent?: string;
  evidence?: object;
};

const ELIGIBLE = {
  eligible: true,
  condition: "many_to_few",
  chapter_count: chapters.length,
  track_count: 2,
  existing_evidence: null,
};

// Every gate the pure predicate can refuse on. Backend eligibility rules are
// covered in Rust; here each shape only has to keep the provider untouched.
const BLOCKED: Fixture[] = [
  {
    key: "auto-count-off",
    title: "Count Off Fixture",
    availability: { ...ELIGIBLE, eligible: false, condition: "count_off" },
    consent: "groq",
  },
  {
    key: "auto-one-to-many",
    title: "One To Many Fixture",
    availability: {
      ...ELIGIBLE,
      eligible: false,
      condition: "one_to_many",
      chapter_count: 2,
      track_count: 5,
    },
    consent: "groq",
  },
  {
    key: "auto-zero-chapters",
    title: "Zero Chapters Fixture",
    availability: { ...ELIGIBLE, eligible: false, chapter_count: 0 },
    consent: "groq",
  },
  {
    key: "auto-zero-tracks",
    title: "Zero Tracks Fixture",
    availability: { ...ELIGIBLE, eligible: false, track_count: 0 },
    consent: "groq",
  },
  {
    key: "auto-not-chapters-heavy",
    title: "Not Chapters Heavy Fixture",
    availability: {
      ...ELIGIBLE,
      eligible: false,
      chapter_count: 2,
      track_count: 2,
    },
    consent: "groq",
  },
  {
    key: "auto-missing-key",
    title: "Missing Key Fixture",
    availability: { ...ELIGIBLE, key_present: false },
    consent: "groq",
  },
  {
    key: "auto-absent-consent",
    title: "Absent Consent Fixture",
    availability: { ...ELIGIBLE },
  },
  {
    key: "auto-stale-consent",
    title: "Stale Consent Fixture",
    availability: { ...ELIGIBLE },
    consent: "open_ai",
  },
];

const FIXTURES: Fixture[] = [
  {
    key: AUTO_KEY,
    title: "Auto Detection Fixture",
    availability: { ...ELIGIBLE },
    consent: "groq",
  },
  {
    // Evidence comes from the seeded matcher decision, so a reset clears it and
    // only the one-shot suppression can keep auto mode from restarting.
    key: EVIDENCE_KEY,
    title: "Existing Evidence Fixture",
    availability: { ...ELIGIBLE },
    consent: "groq",
    evidence: detectedPreview,
  },
  ...BLOCKED,
];

function fixtureScript(autoDetectStart: boolean): string {
  return `;(() => {
    const fixtures = ${JSON.stringify(FIXTURES)};
    const chapters = ${JSON.stringify(chapters)};
    const mapping = ${JSON.stringify(mapping)};
    window.__transcriptionPreferences__ = {
      provider_id: "groq",
      auto_detect_start: ${JSON.stringify(autoDetectStart)},
    };
    window.__transcriptionKeys__ = { groq: true, open_ai: true };
    window.__transcriptionConsents__ = {};
    window.__matcherInspectionByProject__ = {};
    window.__matcherDecisionByProject__ = {};
    window.__detectionAvailabilityByProject__ = {};
    for (const fixture of fixtures) {
      window.__pickerState__.chaptersByProject[fixture.key] = chapters;
      window.__matcherInspectionByProject__[fixture.key] = {
        title: fixture.title,
        chapter_count: fixture.availability.chapter_count,
        track_count: fixture.availability.track_count,
        condition: fixture.availability.condition || "count_off",
        options: ["split_proportional", "single_lesson", "cancel"],
        preselect: "split_proportional",
        bucket_preview: null,
      };
      window.__detectionAvailabilityByProject__[fixture.key] = fixture.availability;
      if (fixture.consent) {
        window.__transcriptionConsents__[fixture.key] = fixture.consent;
      }
      if (fixture.evidence) {
        window.__matcherDecisionByProject__[fixture.key] = {
          condition: "many_to_few",
          response: "split_proportional",
          chapter_count: chapters.length,
          track_count: 2,
          user_overrode: false,
          decided_at: ${JSON.stringify(DETECTED_AT)},
          detection: fixture.evidence,
        };
        window.__mappingState__.seed(fixture.key, mapping);
      }
    }
  })();`;
}

function detectionCalls(page: Page): Promise<number> {
  return page.evaluate(() => window.__detectionStartCalls__ ?? 0);
}

async function emit(page: Page, payload: object): Promise<void> {
  await page.evaluate((event) => window.__emitEvent__("job", event), payload);
}

/** Auto mode starts on mount, so gates and results must be seeded pre-goto. */
async function seedRun(
  page: Page,
  seed: {
    hold?: boolean;
    result?: DetectStartResult;
    error?: AppError | Error;
  },
): Promise<void> {
  await page.addInitScript((next: typeof seed) => {
    if (next.hold) {
      window.__detectionGate__ = new Promise((resolve) => {
        window.__releaseDetection__ = resolve;
      });
    }
    if (next.result) window.__detectionResult__ = next.result;
    if (next.error) window.__detectionCommandError__ = next.error;
  }, seed);
}

async function autoStarted(page: Page): Promise<string> {
  // The resolver shell renders while hydrating, so waiting on it absorbs the
  // dev server's first-navigation route compile before the start poll.
  await expect(
    page.getByRole("heading", { name: "Resolve mismatch" }),
  ).toBeVisible({ timeout: 20_000 });
  await expect.poll(() => detectionCalls(page)).toBe(1);
  const jobId = await page.evaluate(
    () => window.__detectionStartArgs__?.[0]?.jobId,
  );
  if (!jobId) throw new Error("no detection start recorded");
  return jobId;
}

/** Invoke log filtered to the ordered detection→mapping commands. */
function detectionCommandOrder(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const ordered = [
      "cmd_detect_start_offset",
      "cmd_confirm_detected_range",
      "cmd_confirm_mapping",
    ];
    return window.__invokeLog__.filter((command) => ordered.includes(command));
  });
}

async function expectNoProviderCall(page: Page): Promise<void> {
  expect(await detectionCalls(page)).toBe(0);
  expect(await detectionCommandOrder(page)).toEqual([]);
}

test.describe("gated automatic range detection", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript(true));
  });

  test("eligible auto mode reuses the manual reviewed path", async ({
    page,
  }) => {
    await seedRun(page, { hold: true });
    await page.goto(`/match/${AUTO_KEY}`);
    const jobId = await autoStarted(page);

    await emit(page, {
      kind: "Started",
      job_id: jobId,
      stage: { kind: "detecting_start" },
    });
    await emit(page, {
      kind: "DetectionProgress",
      job_id: jobId,
      pct: 0.1,
      phase: "title_check",
    });
    await expect(page.getByRole("status")).toContainText(
      "Checking embedded titles",
    );
    await emit(page, {
      kind: "Result",
      job_id: jobId,
      ok: true,
      payload: { kind: "detected", preview: detectedPreview },
    });

    const preview = page.getByTestId("detection-range-preview");
    await expect(preview).toContainText("Chapter 2 · Arrival");
    await expect(preview).toContainText("Chapter 3 · Return");

    await page.getByRole("button", { name: "Confirm detected range" }).click();
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`/match/${AUTO_KEY}$`));
    expect(await detectionCalls(page)).toBe(1);
    // The mapping grid is a gate: /run stays unreachable until it is confirmed.
    expect(await detectionCommandOrder(page)).toEqual([
      "cmd_detect_start_offset",
      "cmd_confirm_detected_range",
    ]);

    await page.getByTestId("mapping-continue").click();
    await expect(page).toHaveURL(new RegExp(`/run/${AUTO_KEY}`));
    expect(await detectionCommandOrder(page)).toEqual([
      "cmd_detect_start_offset",
      "cmd_confirm_detected_range",
      "cmd_confirm_mapping",
    ]);
  });

  test("the app toggle off keeps an eligible project manual", async ({
    page,
  }) => {
    await page.addInitScript(fixtureScript(false));
    await page.goto(`/match/${AUTO_KEY}`);
    await expect(page.getByText("Auto Detection Fixture")).toBeVisible({
      timeout: 15_000,
    });
    await expect(
      page.getByRole("button", { name: "Detect audio's text range" }),
    ).toBeVisible();
    await expectNoProviderCall(page);
  });

  for (const fixture of BLOCKED) {
    test(`no provider upload starts for ${fixture.key}`, async ({ page }) => {
      await page.goto(`/match/${fixture.key}`);
      await expect(page.getByText(fixture.title)).toBeVisible({
        timeout: 15_000,
      });
      await expect(
        page.getByRole("button", { name: "Split by embedded chapters" }),
      ).toBeVisible();
      await expect(page.getByRole("progressbar")).toHaveCount(0);
      await expectNoProviderCall(page);
    });
  }

  test("existing evidence is reused instead of detected again", async ({
    page,
  }) => {
    await page.goto(`/match/${EVIDENCE_KEY}`);
    const evidence = page.getByTestId("detection-evidence-panel");
    await expect(evidence).toBeVisible({ timeout: 15_000 });
    await expect(evidence).toContainText("Chapter 2 · Arrival");
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expectNoProviderCall(page);
  });

  test("a reset never becomes the trigger for the next auto run", async ({
    page,
  }) => {
    await page.goto(`/match/${EVIDENCE_KEY}`);
    await expect(page.getByTestId("detection-evidence-panel")).toBeVisible({
      timeout: 15_000,
    });
    await page.getByRole("button", { name: "Reset detected range" }).click();
    await page.getByRole("button", { name: "Confirm reset" }).click();

    await expect(
      page.getByRole("button", { name: "Detect audio's text range" }),
    ).toBeVisible();
    await expect(page.getByRole("progressbar")).toHaveCount(0);
    await expectNoProviderCall(page);
  });

  test("an inconclusive auto result explains itself and keeps manual choices", async ({
    page,
  }) => {
    await seedRun(page, { result: lowConfidence });
    await page.goto(`/match/${AUTO_KEY}`);
    await autoStarted(page);

    await expect(page.getByText("Detection needs refinement")).toBeVisible();
    await expect(
      page.getByRole("group", { name: "Start chapter" }),
    ).toContainText("Arrival");
    await expect(
      page.getByRole("button", { name: "Split by embedded chapters" }),
    ).toBeVisible();
    expect(await detectionCalls(page)).toBe(1);
  });

  test("an auto rate limit banners once and never retries itself", async ({
    page,
  }) => {
    await seedRun(page, {
      error: {
        kind: "Transcribe",
        message: { kind: "rate_limit", message: "rate limited" },
      },
    });
    await page.goto(`/match/${AUTO_KEY}`);
    await autoStarted(page);

    await expect(page.getByRole("alert")).toContainText(
      "rate limit was reached",
    );
    const assist = page.getByTestId("detection-assist");
    await expect(
      assist.getByRole("link", { name: "Switch transcription provider" }),
    ).toBeVisible();
    await expect(
      assist.getByRole("button", { name: "Try detection again" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Split by embedded chapters" }),
    ).toBeVisible();
    expect(await detectionCalls(page)).toBe(1);
  });
});
