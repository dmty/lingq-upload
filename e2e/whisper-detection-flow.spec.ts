import { expect, test, type Page } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const PROJECT_KEY = "detection-flow-fixture";
const OTHER_PROJECT_KEY = "detection-flow-other";
const START_ID = "spine:arrival";
const END_ID = "spine:return";

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

const inspection = {
  title: "Range Detection Fixture",
  chapter_count: chapters.length,
  track_count: 2,
  condition: "many_to_few",
  options: ["split_proportional", "single_lesson", "cancel"],
  preselect: "split_proportional",
  bucket_preview: null,
};

const titlePreview = {
  provider_id: null,
  align_source: "title",
  range: { start_chapter_id: START_ID, end_chapter_id: END_ID },
  confidence: 0.93,
  transcript_head_preview: null,
  transcript_tail_preview: null,
  detected_at: "2026-08-12T00:00:00Z",
};

const transcriptPreview = {
  provider_id: "groq",
  align_source: "transcript",
  range: { start_chapter_id: START_ID, end_chapter_id: END_ID },
  confidence: 0.8123,
  transcript_head_preview: "始まり 🐉 café — arrival",
  transcript_tail_preview: "帰還 🌊 fin — return",
  detected_at: "2026-08-12T00:01:00Z",
};

function fixtureScript(): string {
  return `;(() => {
    const chapters = ${JSON.stringify(chapters)};
    const inspection = ${JSON.stringify(inspection)};
    window.__pickerState__.chaptersByProject[${JSON.stringify(PROJECT_KEY)}] = chapters;
    window.__pickerState__.chaptersByProject[${JSON.stringify(OTHER_PROJECT_KEY)}] = chapters;
    window.__matcherInspectionByProject__ = {
      ${JSON.stringify(PROJECT_KEY)}: inspection,
      ${JSON.stringify(OTHER_PROJECT_KEY)}: { ...inspection, title: "Other Detection Fixture" },
    };
    window.__transcriptionKeys__ = { groq: true };
    window.__transcriptionConsents__ = {
      ${JSON.stringify(PROJECT_KEY)}: "groq",
      ${JSON.stringify(OTHER_PROJECT_KEY)}: "groq",
    };
    window.__detectionAvailabilityByProject__ = {
      ${JSON.stringify(PROJECT_KEY)}: {
        eligible: true,
        condition: "many_to_few",
        chapter_count: chapters.length,
        track_count: 2,
        existing_evidence: null,
      },
      ${JSON.stringify(OTHER_PROJECT_KEY)}: {
        eligible: true,
        condition: "many_to_few",
        chapter_count: chapters.length,
        track_count: 2,
        existing_evidence: null,
      },
    };
  })();`;
}

async function holdDetection(page: Page): Promise<void> {
  await page.evaluate(() => {
    window.__detectionGate__ = new Promise((resolve) => {
      window.__releaseDetection__ = resolve;
    });
  });
}

async function startDetection(
  page: Page,
  projectKey = PROJECT_KEY,
): Promise<string> {
  await page.goto(`/match/${projectKey}`);
  await expect(
    page.getByText(
      projectKey === PROJECT_KEY
        ? "Range Detection Fixture"
        : "Other Detection Fixture",
    ),
  ).toBeVisible();
  await holdDetection(page);
  await page.getByRole("button", { name: "Detect audio's text range" }).click();
  await expect
    .poll(() => page.evaluate(() => window.__detectionStartArgs__?.length ?? 0))
    .toBe(1);
  return page.evaluate(() => window.__detectionStartArgs__[0].jobId);
}

async function emit(page: Page, payload: object): Promise<void> {
  await page.evaluate((event) => window.__emitEvent__("job", event), payload);
}

async function emitDetected(
  page: Page,
  jobId: string,
  preview: typeof titlePreview,
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

test.describe("detected text range flow", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(fixtureScript());
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

  test("renders title and collapsed Unicode transcript previews while Refine stays local", async ({
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
    await expect
      .poll(() => page.evaluate(() => window.__detectionStartArgs__.length))
      .toBe(2);
    jobId = await page.evaluate(() => window.__detectionStartArgs__[1].jobId);
    await emitDetected(page, jobId, transcriptPreview);

    await expect(preview).toContainText("Transcription");
    await expect(preview).toContainText("81%");
    const details = preview.locator("details");
    await expect(details).toHaveCount(2);
    await expect(details.first()).not.toHaveAttribute("open", "");
    await expect(
      preview.getByText(transcriptPreview.transcript_head_preview),
    ).not.toBeVisible();
    await details.first().locator("summary").click();
    await expect(
      preview.getByText(transcriptPreview.transcript_head_preview),
    ).toBeVisible();
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
    await expect(page.getByText("Arrival")).toBeVisible();
    await expect(page.getByRole("alert")).toHaveCount(0);

    await page.getByRole("button", { name: "Refine" }).click();
    await holdDetection(page);
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();
    await expect
      .poll(() => page.evaluate(() => window.__detectionStartArgs__.length))
      .toBe(2);
    jobId = await page.evaluate(() => window.__detectionStartArgs__[1].jobId);
    await page.evaluate(() => {
      window.__detectionCommandError__ = {
        kind: "Transcribe",
        message: { kind: "rate_limit" },
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
    await page.evaluate(() => window.__releaseDetection__());
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
        message: { kind: "api_key" },
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

  test("confirms stable IDs into MappingGrid and routes only from mapping confirmation", async ({
    page,
  }) => {
    const jobId = await startDetection(page);
    await emitDetected(page, jobId, transcriptPreview);

    await page.getByRole("button", { name: "Confirm detected range" }).click();
    await expect(page).toHaveURL(new RegExp(`/match/${PROJECT_KEY}$`));
    await expect(page.getByTestId("mapping-grid")).toBeVisible();

    const args = await page.evaluate(
      () => window.__confirmDetectedRangeCalls__[0],
    );
    expect(args.selectedRange).toEqual({
      start_chapter_id: START_ID,
      end_chapter_id: END_ID,
    });
    expect(args.evidence.range).toEqual(args.selectedRange);
    expect(args.evidence.confidence).toBe(transcriptPreview.confidence);

    await page.getByTestId("mapping-continue").click();
    await expect(page).toHaveURL(new RegExp(`/run/${PROJECT_KEY}`));
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
