import { expect, test, type Page } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const TRANSCRIPT_KEY = "evidence-transcript";
const TITLE_KEY = "evidence-title";
const RECEIPTS_KEY = "evidence-receipts";
const STALE_KEY = "evidence-stale";

const START_ID = "spine:arrival";
const END_ID = "spine:return";
const MISSING_ID = "spine:removed";
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
  { id: "spine:notes", order: 3, title: "Notes", body: "", kind: "back_matter" },
];

const transcriptEvidence = {
  provider_id: "groq",
  align_source: "transcript",
  range: { start_chapter_id: START_ID, end_chapter_id: END_ID },
  confidence: 0.8123,
  transcript_head_preview: "始まり 🐉 café — arrival",
  transcript_tail_preview: "帰還 🌊 fin — return",
  detected_at: DETECTED_AT,
  atom_starts: [],
};

const titleEvidence = {
  provider_id: null,
  align_source: "title",
  range: { start_chapter_id: START_ID, end_chapter_id: END_ID },
  confidence: 0.93,
  transcript_head_preview: null,
  transcript_tail_preview: null,
  detected_at: DETECTED_AT,
  atom_starts: [],
};

const staleEvidence = {
  ...transcriptEvidence,
  range: { start_chapter_id: START_ID, end_chapter_id: MISSING_ID },
};

const EVIDENCE_BY_PROJECT: Record<string, object> = {
  [TRANSCRIPT_KEY]: transcriptEvidence,
  [TITLE_KEY]: titleEvidence,
  [RECEIPTS_KEY]: transcriptEvidence,
  [STALE_KEY]: staleEvidence,
};

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

const inspection = {
  title: "Evidence Fixture",
  chapter_count: chapters.length,
  track_count: 2,
  condition: "many_to_few",
  options: ["split_proportional", "single_lesson", "cancel"],
  preselect: "split_proportional",
  bucket_preview: null,
};

function fixtureScript(): string {
  return `;(() => {
    const chapters = ${JSON.stringify(chapters)};
    const evidenceByProject = ${JSON.stringify(EVIDENCE_BY_PROJECT)};
    const mapping = ${JSON.stringify(mapping)};
    const inspection = ${JSON.stringify(inspection)};
    window.__matcherInspectionByProject__ = {};
    window.__matcherDecisionByProject__ = {};
    window.__detectionAvailabilityByProject__ = {};
    window.__receiptsByProject__ = {
      ${JSON.stringify(RECEIPTS_KEY)}: [
        { chapter_index: 0, lesson_id: 51, degraded: false, uploaded_at: ${JSON.stringify(DETECTED_AT)} },
      ],
    };
    window.__transcriptionConsents__ = {};
    window.__transcriptionKeys__ = { groq: true };
    for (const key of Object.keys(evidenceByProject)) {
      window.__pickerState__.chaptersByProject[key] = chapters;
      window.__matcherInspectionByProject__[key] = inspection;
      window.__matcherDecisionByProject__[key] = {
        condition: "many_to_few",
        response: "split_proportional",
        chapter_count: chapters.length,
        track_count: 2,
        user_overrode: false,
        decided_at: ${JSON.stringify(DETECTED_AT)},
        detection: evidenceByProject[key],
      };
      window.__mappingState__.seed(key, mapping);
      window.__transcriptionConsents__[key] = "groq";
      window.__detectionAvailabilityByProject__[key] = {
        eligible: true,
        condition: "many_to_few",
        chapter_count: chapters.length,
        track_count: 2,
        existing_evidence: null,
      };
    }
  })();`;
}

function panel(page: Page) {
  return page.getByTestId("detection-evidence-panel");
}

async function detectionCalls(page: Page): Promise<number> {
  return page.evaluate(() => window.__detectionStartCalls__ ?? 0);
}

async function resetCalls(page: Page): Promise<number> {
  return page.evaluate(() => window.__resetDetectionCalls__?.length ?? 0);
}

test.describe("confirmed detection evidence and reset", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
  });

  test("renders every confirmed field in its own panel above the grid", async ({
    page,
  }) => {
    await page.goto(`/match/${TRANSCRIPT_KEY}`);
    // First navigation of the file pays the dev server's route compile.
    await expect(page.getByTestId("mapping-grid")).toBeVisible({
      timeout: 15_000,
    });

    const evidence = panel(page);
    await expect(evidence).toContainText("Chapter 2 · Arrival");
    await expect(evidence).toContainText("Chapter 3 · Return");
    await expect(evidence).toContainText("Transcription");
    await expect(evidence).toContainText("81%");
    await expect(evidence).toContainText("Groq");

    const stamp = evidence.locator("time");
    await expect(stamp).toHaveAttribute("datetime", DETECTED_AT);
    await expect(stamp).not.toHaveText(DETECTED_AT);

    const details = evidence.locator("details");
    await expect(details).toHaveCount(2);
    await expect(details.first()).not.toHaveAttribute("open", "");
    await expect(
      evidence.getByText(transcriptEvidence.transcript_head_preview),
    ).not.toBeVisible();
    await details.first().locator("summary").click();
    await expect(
      evidence.getByText(transcriptEvidence.transcript_head_preview),
    ).toBeVisible();

    // Its own panel, not the unresolved-mismatch evidence card, and not nested
    // inside the grid.
    await expect(
      page.locator('[data-testid="mapping-grid"] [data-testid="detection-evidence-panel"]'),
    ).toHaveCount(0);
    await expect(
      page.getByText(/audio chapters found in the file/),
    ).toHaveCount(0);
    expect(
      await page.evaluate(() => {
        const own = document.querySelector(
          '[data-testid="detection-evidence-panel"]',
        );
        const grid = document.querySelector('[data-testid="mapping-grid"]');
        if (!own || !grid) return 0;
        return own.compareDocumentPosition(grid) &
          Node.DOCUMENT_POSITION_FOLLOWING;
      }),
    ).toBeGreaterThan(0);
  });

  test("title matching states no provider upload and hides transcript previews", async ({
    page,
  }) => {
    await page.goto(`/match/${TITLE_KEY}`);
    const evidence = panel(page);
    await expect(evidence).toContainText("Embedded titles");
    await expect(evidence).toContainText("93%");
    await expect(evidence).toContainText("No provider upload");
    await expect(evidence.locator("details")).toHaveCount(0);
  });

  test("reset takes two steps, returns to mismatch resolution, and detects nothing", async ({
    page,
  }) => {
    await page.goto(`/match/${TRANSCRIPT_KEY}`);
    const trigger = page.getByRole("button", { name: "Reset detected range" });
    await trigger.click();

    const confirmReset = page.getByRole("button", { name: "Confirm reset" });
    await expect(confirmReset).toBeVisible();
    expect(await resetCalls(page)).toBe(0);

    await page.getByRole("button", { name: "Keep detected range" }).click();
    await expect(confirmReset).toHaveCount(0);
    await expect(trigger).toBeFocused();
    expect(await resetCalls(page)).toBe(0);

    await trigger.click();
    await page.getByRole("button", { name: "Confirm reset" }).click();

    await expect(
      page.getByRole("heading", { name: "Resolve mismatch" }),
    ).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`/match/${TRANSCRIPT_KEY}$`));
    await expect(panel(page)).toHaveCount(0);
    await expect(page.getByTestId("mapping-grid")).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Detect audio's text range" }),
    ).toBeVisible();
    expect(await resetCalls(page)).toBe(1);
    expect(await detectionCalls(page)).toBe(0);

    // Consent, provider preference, and stored keys are untouched by a reset.
    const preserved = await page.evaluate(async (key) => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      return {
        keyPresent: await invoke("cmd_transcribe_key_present", {
          provider: "groq",
        }),
        preferences: await invoke("cmd_get_transcription_preferences"),
        availability: await invoke("cmd_detection_availability", {
          projectId: { content_hash: key },
        }),
      };
    }, TRANSCRIPT_KEY);
    expect(preserved.keyPresent).toBe(true);
    expect(preserved.preferences.provider_id).toBe("groq");
    expect(preserved.availability.consent_matches).toBe(true);
  });

  test("a failed reset keeps the evidence and restores focus", async ({
    page,
  }) => {
    await page.goto(`/match/${TRANSCRIPT_KEY}`);
    await page.evaluate(() => {
      window.__resetDetectionError__ = {
        kind: "Unsupported",
        message: "confirmed detection changed; reload the project and try again",
      };
    });
    const trigger = page.getByRole("button", { name: "Reset detected range" });
    await trigger.click();
    await page.getByRole("button", { name: "Confirm reset" }).click();

    await expect(panel(page)).toContainText("Chapter 2 · Arrival");
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
    await expect(panel(page).getByRole("alert")).toContainText(
      "confirmed detection changed",
    );
    await expect(trigger).toBeFocused();
  });

  test("reset is unavailable once uploads have begun", async ({ page }) => {
    await page.goto(`/match/${RECEIPTS_KEY}`);
    const evidence = panel(page);
    await expect(evidence).toContainText("Chapter 2 · Arrival");
    await expect(
      evidence.getByRole("button", { name: "Reset detected range" }),
    ).toHaveCount(0);
    await expect(evidence).toContainText(
      "Uploads have started for this project, so the detected range can no longer be reset.",
    );
  });

  test("an unresolvable boundary shows the stable ID and blocks Continue", async ({
    page,
  }) => {
    await page.goto(`/match/${STALE_KEY}`);
    const evidence = panel(page);
    await expect(evidence).toContainText(`Chapter unavailable · ${MISSING_ID}`);
    await expect(page.getByTestId("mapping-continue")).toBeDisabled();
    // Reset stays available: it is the way out of stale evidence.
    await expect(
      evidence.getByRole("button", { name: "Reset detected range" }),
    ).toBeEnabled();
  });
});
