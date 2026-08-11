import { expect, test } from "@playwright/test";

import { tauriStubInitScriptFor } from "./setup/tauri-stub";

const ELIGIBLE = "consent-eligible";
const INELIGIBLE = "consent-ineligible";
const MISSING_KEY = "consent-missing-key";

function consentScenarioScript(): string {
  const inspection = {
    title: "The Test Book",
    chapter_count: 12,
    track_count: 3,
    condition: "many_to_few",
    options: ["split_proportional", "single_lesson", "cancel"],
    preselect: "split_proportional",
    bucket_preview: null,
  };
  return `;(() => {
    window.__matcherInspection__ = ${JSON.stringify(inspection)};
    window.__transcriptionKeys__ = { groq: true, open_ai: true };
    window.__detectionAvailabilityByProject__ = {
      ${JSON.stringify(ELIGIBLE)}: {
        eligible: true,
        condition: "many_to_few",
        chapter_count: 12,
        track_count: 3,
        existing_evidence: null,
      },
      ${JSON.stringify(INELIGIBLE)}: {
        eligible: false,
        condition: "count_off",
        chapter_count: 3,
        track_count: 3,
        existing_evidence: null,
      },
      ${JSON.stringify(MISSING_KEY)}: {
        eligible: true,
        condition: "many_to_few",
        chapter_count: 12,
        track_count: 3,
        key_present: false,
        existing_evidence: null,
      },
    };
  })();`;
}

async function invokeCount(page: import("@playwright/test").Page, cmd: string) {
  return page.evaluate(
    (command) =>
      ((window as any).__invokeLog__ as string[]).filter(
        (entry) => entry === command,
      ).length,
    cmd,
  );
}

test.describe("detected-range transcription consent", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(consentScenarioScript());
  });

  test("eligible mismatches show a separate assist without changing manual responses", async ({
    page,
  }) => {
    await page.goto(`/match/${ELIGIBLE}`);
    await expect(page.getByText("The Test Book")).toBeVisible({
      timeout: 10_000,
    });

    await expect(page.getByTestId("detection-assist")).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Detect audio's text range" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: /Split by embedded chapters/ }),
    ).toBeVisible();
    await expect(
      page.getByText(
        "Group text chapters proportionally so each audio chapter gets its share.",
        { exact: true },
      ),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: /Single lesson/ }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Confirm" })).toBeVisible();
  });

  test("ineligible mismatches omit the assist", async ({ page }) => {
    await page.goto(`/match/${INELIGIBLE}`);

    await expect(page.getByTestId("detection-assist")).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: /Split by embedded chapters/ }),
    ).toBeVisible();
  });

  test("missing provider keys offer Settings without blocking manual resolution", async ({
    page,
  }) => {
    await page.goto(`/match/${MISSING_KEY}`);

    const assist = page.getByTestId("detection-assist");
    await expect(assist).toContainText("Detect audio's text range");
    await expect(
      assist.getByRole("link", { name: "Open transcription settings" }),
    ).toHaveAttribute("href", "/settings");
    await expect(
      page.getByRole("button", { name: /Split by embedded chapters/ }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Confirm" })).toBeEnabled();
  });

  test("the Groq modal gives the complete provider-specific disclosure", async ({
    page,
  }) => {
    await page.goto(`/match/${ELIGIBLE}`);
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();

    const dialog = page.getByRole("dialog", {
      name: "Allow Groq transcription?",
    });
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText(
      "Stage A checks embedded audio titles locally and is free.",
    );
    await expect(dialog).toContainText("two 30-second clips");
    await expect(dialog).toContainText("one retry per side");
    await expect(dialog).toContainText("maximum four calls / two minutes");
    await expect(dialog).toContainText(
      "optional book title and author prompt metadata",
    );
    await expect(dialog).toContainText(
      "Free-tier eligible; limits depend on your account/tier; current paid reference $0.04/hour",
    );
    await expect(dialog).toContainText("OS keychain");
    await expect(
      dialog.getByRole("link", { name: "Groq data policy" }),
    ).toHaveAttribute("href", "https://console.groq.com/docs/your-data");
    await expect(
      dialog.getByRole("link", { name: "Groq data policy" }),
    ).toHaveAttribute("rel", "noopener noreferrer");
  });

  test("focus wraps within the modal", async ({ page }) => {
    await page.goto(`/match/${ELIGIBLE}`);
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();

    const cancel = page.getByRole("button", { name: "Cancel" });
    const accept = page.getByRole("button", { name: "Accept and continue" });
    await expect(cancel).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await expect(accept).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(cancel).toBeFocused();
  });

  test("Escape cancels without consent or detection and restores trigger focus", async ({
    page,
  }) => {
    await page.goto(`/match/${ELIGIBLE}`);
    const trigger = page.getByRole("button", {
      name: "Detect audio's text range",
    });
    await trigger.click();
    await page.keyboard.press("Escape");

    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(trigger).toBeFocused();
    expect(await invokeCount(page, "cmd_accept_transcribe_consent")).toBe(0);
    expect(await invokeCount(page, "cmd_detect_start_offset")).toBe(0);
  });

  test("Cancel makes no consent or detection call and restores trigger focus", async ({
    page,
  }) => {
    await page.goto(`/match/${ELIGIBLE}`);
    const trigger = page.getByRole("button", {
      name: "Detect audio's text range",
    });
    await trigger.click();
    await page.getByRole("button", { name: "Cancel" }).click();

    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(trigger).toBeFocused();
    expect(await invokeCount(page, "cmd_accept_transcribe_consent")).toBe(0);
    expect(await invokeCount(page, "cmd_detect_start_offset")).toBe(0);
  });

  test("accept saves consent before refreshing and starts no detection in Task 17", async ({
    page,
  }) => {
    await page.goto(`/match/${ELIGIBLE}`);
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();
    await page.getByRole("button", { name: "Accept and continue" }).click();

    await expect(page.getByRole("dialog")).toHaveCount(0);
    const log = await page.evaluate(
      () => (window as any).__invokeLog__ as string[],
    );
    const acceptAt = log.lastIndexOf("cmd_accept_transcribe_consent");
    const refreshAt = log.lastIndexOf("cmd_detection_availability");
    expect(acceptAt).toBeGreaterThan(-1);
    expect(refreshAt).toBeGreaterThan(acceptAt);
    expect(await invokeCount(page, "cmd_detect_start_offset")).toBe(0);
  });

  test("consent failure stays open with an alert and starts no detection", async ({
    page,
  }) => {
    await page.goto(`/match/${ELIGIBLE}`);
    await page.evaluate(() => {
      (window as any).__failNextTranscribeConsent__ = true;
    });
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();
    await page.getByRole("button", { name: "Accept and continue" }).click();

    await expect(page.getByRole("dialog")).toBeVisible();
    await expect(page.getByRole("alert")).toHaveText(
      "Could not save transcription consent.",
    );
    expect(await invokeCount(page, "cmd_detect_start_offset")).toBe(0);
  });

  test("switching providers requires consent for the newly active provider", async ({
    page,
  }) => {
    await page.goto(`/match/${ELIGIBLE}`);
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();
    await page.getByRole("button", { name: "Accept and continue" }).click();
    await expect(page.getByRole("dialog")).toHaveCount(0);

    await page.goto("/settings");
    await page.getByLabel("OpenAI", { exact: true }).check();
    await page.goto(`/match/${ELIGIBLE}`);
    await page
      .getByRole("button", { name: "Detect audio's text range" })
      .click();

    const dialog = page.getByRole("dialog", {
      name: "Allow OpenAI transcription?",
    });
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText(
      "No free tier; current reference $0.006/min",
    );
    await expect(
      dialog.getByRole("link", { name: "OpenAI data policy" }),
    ).toHaveAttribute(
      "href",
      "https://platform.openai.com/docs/models/default-usage-policies-by-endpoint",
    );
  });
});
