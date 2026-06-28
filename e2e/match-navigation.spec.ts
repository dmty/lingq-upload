import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

// The match page component is reused across `/match/:projectId` navigations.
// Every project-scoped state piece (title, counts, bucket preview, selected
// response) must re-seed when the param changes — otherwise the previous
// book's "Proposed split" panel and counters bleed into the next book.

const PROJECT_A = "project-a";
const PROJECT_B = "project-b";

function multiProjectScript(): string {
  const inspectionA = {
    title: "Book A — Toki",
    chapter_count: 9,
    track_count: 5,
    condition: "count_off",
    options: ["split_proportional", "cancel"],
    preselect: "split_proportional",
    bucket_preview: [
      {
        atomTitle: "A Atom 1",
        atomDurationSec: 600,
        textRangeStart: 0,
        textRangeEnd: 2,
        charsPerSec: 12.3,
      },
      {
        atomTitle: "A Atom 2",
        atomDurationSec: 700,
        textRangeStart: 2,
        textRangeEnd: 5,
        charsPerSec: 12.8,
      },
    ],
  };
  const inspectionB = {
    title: "Book B — Silent Witch",
    chapter_count: 85,
    track_count: 6,
    condition: "count_off",
    options: ["single_lesson", "cancel"],
    preselect: "cancel",
    bucket_preview: null,
  };
  return `;(() => {
    window.__matcherInspectionByProject__ = {
      ${JSON.stringify(PROJECT_A)}: ${JSON.stringify(inspectionA)},
      ${JSON.stringify(PROJECT_B)}: ${JSON.stringify(inspectionB)},
    };
  })();`;
}

test.describe("match navigation", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(multiProjectScript());
  });

  test("switching projects refreshes the mismatch resolver state", async ({
    page,
  }) => {
    await page.goto(`/match/${PROJECT_A}`);

    // Project A: title, counts, and Proposed split all visible.
    await expect(page.getByRole("heading", { name: "Resolve mismatch" }))
      .toBeVisible();
    await expect(page.getByText("Book A — Toki")).toBeVisible();
    await expect(page.getByText("Proposed split")).toBeVisible();
    await expect(page.getByText("A Atom 1")).toBeVisible();

    // Cross-project navigation: simulates Back → click another book in the
    // library. SvelteKit reuses the page component across the same route.
    await page.goto(`/match/${PROJECT_B}`);

    // Project B: title and counts reflect B, and A's bucket preview is gone.
    await expect(page.getByText("Book B — Silent Witch")).toBeVisible();
    await expect(page.getByText("Proposed split")).toHaveCount(0);
    await expect(page.getByText("A Atom 1")).toHaveCount(0);
    await expect(page.getByText("A Atom 2")).toHaveCount(0);
    await expect(page.getByText("Book A — Toki")).toHaveCount(0);
  });
});
