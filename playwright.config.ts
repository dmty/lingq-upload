import { defineConfig, devices } from "@playwright/test";

// Headless, single-worker, zero-retries by default. The webServer hook spins
// up Vite (`bun run dev`) on port 1420 — the Tauri dev port. CI can override
// retries via the CI env var.
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
