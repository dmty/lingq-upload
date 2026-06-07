import { defineConfig, devices } from "@playwright/test";

// Headless, single-worker, zero-retries by default. The webServer hook spins
// up Vite (`bun run dev`) on port 1420 — the Tauri dev port. CI can override
// retries via the CI env var.
//
// Vite has no Tauri runtime, so any `commands.*` call from the page would
// throw on mount. Specs install an init script (`e2e/setup/tauri-stub.ts`)
// via `test.beforeEach` to stub the IPC for the empty-state path. Extend the
// stub's handler map as the test surface grows.
export default defineConfig({
    testDir: "e2e",
    fullyParallel: false,
    workers: 1,
    retries: process.env.CI ? 1 : 0,
    timeout: 30_000,
    reporter: process.env.CI ? "github" : "list",
    use: {
        baseURL: "http://localhost:1420",
        headless: true,
        trace: "retain-on-failure",
        ...devices["Desktop Chrome"],
    },
    webServer: {
        command: "bun run dev",
        url: "http://localhost:1420",
        timeout: 60_000,
        reuseExistingServer: !process.env.CI,
    },
});
