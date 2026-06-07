import { test } from "@playwright/test";

import { tauriStubInitScript } from "./setup/tauri-stub";

// Keyboard shortcuts on the Library page: `/` focuses search, ↑/↓ cycle row
// focus, Enter activates the focused row's primary action. The current
// tauri-stub returns `{ projects: [] }` for `cmd_library_list`, so the rich
// row UI never renders — the spec stays a skipped placeholder until a seeded
// library fixture is wired into the stub.
test.describe("library keyboard nav", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(tauriStubInitScript);
  });

  test("/ focuses the search input", async () => {
    test.skip(true, "needs seeded library fixture");
  });

  test("ArrowUp / ArrowDown cycle row focus", async () => {
    test.skip(true, "needs seeded library fixture");
  });

  test("Enter activates the focused row", async () => {
    test.skip(true, "needs seeded library fixture");
  });
});
