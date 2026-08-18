import { test as base, type Page } from "@playwright/test";

import { tauriStub } from "./tauri-stub";

// Every spec that drives the app needs the IPC stub installed before the page
// script runs, so it belongs in the fixture rather than in each beforeEach.
export const test = base.extend({
  page: async ({ page }, use, testInfo) => {
    await page.addInitScript(tauriStub, testInfo.workerIndex);
    await use(page);
  },
});

export { expect } from "@playwright/test";

// Init scripts re-run on every navigation, so seeded fixtures survive the
// page.goto that follows.
//
// Typed as Record<string, unknown> rather than Partial<Window> at the
// addInitScript call site: Playwright's Arg type recurses homomorphically
// over every property, and Window's DOM types are self-referential, which
// blows the compiler's instantiation depth limit.
export const seed = (page: Page, fixtures: Partial<Window>) =>
  page.addInitScript<Record<string, unknown>>(
    (f) => Object.assign(window, f),
    fixtures,
  );
