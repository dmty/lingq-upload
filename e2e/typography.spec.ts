import { expect, seed, test } from "./setup/test";
import { libraryEntry } from "./setup/library-fixture";

const entry = libraryEntry("book-1", {
  title: "War and Peace",
  authors: ["Tolstoy"],
  status: "idle",
});

test.describe("typography", () => {
  test.beforeEach(async ({ page }) => {
    await seed(page, { __libraryEntries__: [entry] });
  });

  // The split is content vs chrome, not book vs app: a title in a list row
  // names a book but is still a label, and AppKit sets labels in the system
  // face. Only rendered book prose gets the reading face.
  test("UI is system sans (no phantom Inter), and so are book titles", async ({
    page,
  }) => {
    await page.goto("/library");
    // Without this the probes read an unstyled document ("Times") — the old
    // assertion was loose enough to pass on it.
    await page.waitForLoadState("networkidle");
    const bodyFont = await page.evaluate(
      () => getComputedStyle(document.body).fontFamily,
    );
    expect(bodyFont).not.toContain("Inter");

    const titleFont = await page
      .getByTestId("library-title")
      .evaluate((el) => getComputedStyle(el).fontFamily);
    expect(titleFont).not.toContain("Literata");
    expect(titleFont).toBe(bodyFont);
  });

  // Without this, dropping the webfont entirely would still pass the test
  // above — the reading face has to stay wired for the prose that uses it.
  test("the reading face is still available for book prose", async ({
    page,
  }) => {
    await page.goto("/library");
    // Without this the probes read an unstyled document ("Times") — the old
    // assertion was loose enough to pass on it.
    await page.waitForLoadState("networkidle");
    const serif = await page.evaluate(() => {
      const el = document.createElement("p");
      el.className = "font-serif";
      document.body.append(el);
      const family = getComputedStyle(el).fontFamily;
      el.remove();
      return family;
    });
    expect(serif).toContain("Literata");
  });
});
