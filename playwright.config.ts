import { defineConfig, devices } from "@playwright/test";

// Headless, single-worker, zero-retries by default. The webServer hook spins
// up Vite (`bun run dev`) on port 1420 — the Tauri dev port. CI can override
// retries via the CI env var.
//
// Vite has no Tauri runtime, so any `commands.*` call from the page would
// throw on mount. Specs that drive the app import `test` from
// `e2e/setup/test.ts`; its `page` fixture installs a typed IPC stub
// (`e2e/setup/tauri-stub.ts`) before the page script runs, covering the
// empty-state path. Extend the stub's handler map as the test surface grows.
// A dev server already listening on the default port is reused as-is, which
// silently tests whatever checkout started it. Set E2E_PORT to claim a port of
// this checkout's own when another one is running.
const port = Number(process.env.E2E_PORT ?? 1420);

export default defineConfig({
  testDir: "e2e",
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  timeout: 30_000,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: `http://localhost:${port}`,
    headless: true,
    trace: "retain-on-failure",
    ...devices["Desktop Chrome"],
  },
  webServer: {
    command: `bun run dev --port ${port} --strictPort`,
    url: `http://localhost:${port}`,
    timeout: 60_000,
    reuseExistingServer: !process.env.CI,
  },
});
