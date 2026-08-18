import { expect, seed, test } from "./setup/test";
import type { Project } from "../src/lib/ipc/bindings";

const project: Project = {
  id: { content_hash: "settings-project" },
  sources: { text: { kind: "missing" } },
  settings: { language: "en", collection_title: "Settings Project" },
};

test.describe("transcription settings", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, { __projectByKey__: { "settings-project": project } });
  });

  test("loads app preferences and provider-supplied pricing and policy links", async ({
    page,
  }) => {
    await page.goto("/settings");

    await expect(
      page.getByRole("heading", { name: "Transcription (optional)" }),
    ).toBeVisible();
    await expect(page.getByLabel("Groq", { exact: true })).toBeChecked();
    await expect(
      page.getByLabel("Automatically detect text start"),
    ).not.toBeChecked();

    const groq = page.getByRole("group", { name: "Groq provider" });
    await expect(
      groq.getByText("whisper-large-v3-turbo", { exact: true }),
    ).toBeVisible();
    await expect(
      groq.getByText(
        "Free-tier eligible; limits depend on your account/tier; current paid reference $0.04/hour",
        { exact: true },
      ),
    ).toBeVisible();
    await expect(groq.getByText("No key saved", { exact: true })).toBeVisible();

    const openai = page.getByRole("group", { name: "OpenAI provider" });
    await expect(openai.getByText("whisper-1", { exact: true })).toBeVisible();
    await expect(
      openai.getByText("No free tier; current reference $0.006/min", {
        exact: true,
      }),
    ).toBeVisible();

    await expect(
      page.getByRole("link", { name: "Groq pricing documentation" }),
    ).toHaveAttribute("href", "https://console.groq.com/docs/speech-to-text");
    await expect(
      page.getByRole("link", { name: "Groq data policy" }),
    ).toHaveAttribute("href", "https://console.groq.com/docs/your-data");
    await expect(
      page.getByRole("link", { name: "OpenAI pricing documentation" }),
    ).toHaveAttribute(
      "href",
      "https://developers.openai.com/api/docs/models/whisper-1",
    );
    await expect(
      page.getByRole("link", { name: "OpenAI data policy" }),
    ).toHaveAttribute(
      "href",
      "https://platform.openai.com/docs/models/default-usage-policies-by-endpoint",
    );

    for (const link of await page
      .getByRole("link", { name: /(?:pricing documentation|data policy)$/ })
      .all()) {
      await expect(link).toHaveAttribute("rel", "noopener noreferrer");
    }
  });

  test("saves the complete app preference pair without changing project JSON", async ({
    page,
  }) => {
    await page.goto("/settings");
    const before = await page.evaluate(() =>
      JSON.stringify(window.__projectByKey__),
    );

    await page.getByLabel("OpenAI", { exact: true }).check();
    await expect(page.getByLabel("OpenAI", { exact: true })).toBeEnabled();
    await page.getByLabel("Automatically detect text start").check();
    await expect(
      page.getByLabel("Automatically detect text start"),
    ).toBeEnabled();

    expect(
      await page.evaluate(() => JSON.stringify(window.__projectByKey__)),
    ).toBe(before);

    await page.reload();
    await expect(page.getByLabel("OpenAI", { exact: true })).toBeChecked();
    await expect(
      page.getByLabel("Automatically detect text start"),
    ).toBeChecked();
  });

  test("reverts a preference change when the atomic save fails", async ({
    page,
  }) => {
    await page.goto("/settings");
    await page.evaluate(() => {
      window.__failNextTranscriptionPreferences__ = true;
    });

    await page.getByLabel("OpenAI", { exact: true }).click();

    await expect(
      page.getByText("Could not save transcription preferences."),
    ).toBeVisible();
    await expect(page.getByLabel("Groq", { exact: true })).toBeChecked();
    await expect(page.getByLabel("OpenAI", { exact: true })).not.toBeChecked();
  });

  test("keeps unsaved provider key inputs local to their cards", async ({
    page,
  }) => {
    await page.goto("/settings");
    const groqInput = page.getByPlaceholder("Paste your Groq API key");
    await groqInput.fill("groq-draft");

    await page.getByLabel("OpenAI", { exact: true }).check();
    await expect(
      page.getByPlaceholder("Paste your OpenAI API key"),
    ).toHaveValue("");
    await expect(page.getByText("groq-draft")).toHaveCount(0);

    await page.getByLabel("Groq", { exact: true }).check();
    await expect(groqInput).toHaveValue("groq-draft");
  });

  test("provider keys remain independent and never render values or tails", async ({
    page,
  }) => {
    await page.goto("/settings");
    await page.getByLabel("OpenAI", { exact: true }).check();
    const openaiInput = page.getByPlaceholder("Paste your OpenAI API key");
    await openaiInput.fill("openai-secret");
    await page.getByRole("button", { name: "Save OpenAI key" }).click();
    await expect(openaiInput).toHaveValue("");
    await expect(
      page
        .getByRole("group", { name: "OpenAI provider" })
        .getByText("Key saved", {
          exact: true,
        }),
    ).toBeVisible();

    await page.getByLabel("Groq", { exact: true }).check();
    const groqInput = page.getByPlaceholder("Paste your Groq API key");
    await groqInput.fill("groq-secret");
    await page.getByRole("button", { name: "Save Groq key" }).click();
    await expect(groqInput).toHaveValue("");

    await page.getByLabel("OpenAI", { exact: true }).check();
    await page.getByRole("button", { name: "Remove OpenAI key" }).click();
    await expect(
      page.getByRole("button", { name: "Confirm removing OpenAI key" }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Confirm removing OpenAI key" })
      .click();
    await expect(
      page
        .getByRole("group", { name: "OpenAI provider" })
        .getByText("No key saved", {
          exact: true,
        }),
    ).toBeVisible();

    await page.getByLabel("Groq", { exact: true }).check();
    await expect(
      page
        .getByRole("group", { name: "Groq provider" })
        .getByText("Key saved", {
          exact: true,
        }),
    ).toBeVisible();
    await expect(page.getByText("openai-secret")).toHaveCount(0);
    await expect(page.getByText("groq-secret")).toHaveCount(0);
    await expect(page.getByText(/••••/)).toHaveCount(0);
  });
});
