import { expect, test } from "@playwright/test";

import { tauriStubInitScript } from "./setup/tauri-stub";

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
test.describe("library happy path", () => {
    test.beforeEach(async ({ page }) => {
        // Inject the Tauri IPC stub before any page script runs so calls to
        // `commands.cmd_library_list` (and friends) resolve under Vite.
        await page.addInitScript(tauriStubInitScript);
    });

    test("library empty-state shows Add CTA", async ({ page }) => {
        // `test.skip` only honours the (condition, reason) form when called
        // inside a test body — at module scope it is a no-op.
        test.skip(
            process.env.LINGQ_LIVE !== "1",
            "live LingQ smoke disabled — set LINGQ_LIVE=1 (and LINGQ_STAGING_KEY) to run",
        );

        await page.goto("/library");

        // Empty-state copy lives in src/routes/library/+page.svelte; assert
        // both the header and the Add link are reachable.
        await expect(
            page.getByRole("heading", { name: "Library" }),
        ).toBeVisible();
        await expect(
            page.getByRole("link", { name: /\+ Add/ }),
        ).toBeVisible();
    });
});
