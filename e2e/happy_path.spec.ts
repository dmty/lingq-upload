import { expect, test } from "@playwright/test";

// Live smoke against the real LingQ staging API. Disabled by default; set
// LINGQ_LIVE=1 (and LINGQ_STAGING_KEY for the actual upload steps) to run.
//
// TODO — flesh out the full flow once the live wiring is stable:
//   1. Set the LingQ API key in /settings.
//   2. Add a Calibre source in /add and pick a book.
//   3. Click Create on the matched project.
//   4. Click Start on the Run page.
//   5. Assert receipts populated for every chapter.
//   6. Assert the corresponding LingQ collection exists with N lessons.
//
// The skeleton below only verifies the empty-state CTA renders so the harness
// itself stays green even without live credentials.
test.skip(
    !process.env.LINGQ_LIVE || process.env.LINGQ_LIVE !== "1",
    "live LingQ smoke disabled — set LINGQ_LIVE=1 (and LINGQ_STAGING_KEY) to run",
);

test("library empty-state shows Add CTA", async ({ page }) => {
    await page.goto("/library");

    // The empty-state copy lives in src/routes/library/+page.svelte; assert
    // both the header and the Add link are reachable.
    await expect(page.getByRole("heading", { name: "Library" })).toBeVisible();
    await expect(page.getByRole("link", { name: /\+ Add/ })).toBeVisible();
});
