import { test } from "@playwright/test";

// Pending: requires a Tauri-stub seam that lets a spec inject a project at the
// `Mapped` stage with pre-populated receipts, plus a way to drive job events
// without a real backend. Skipped until the stub grows that surface.

test.skip("Run screen shows queued chips for pre-populated receipts", async ({
  page: _page,
}) => {
  // Seed a project at Mapped with 3 receipts, all lesson_id=null.
  // Navigate to /run/<projectId>.
  // Assert: three chapter rows render with the queued chip state, not done.
});

test.skip("Cancel button reverts in-flight chips to queued", async ({
  page: _page,
}) => {
  // Start a job, observe in_flight chip, click Cancel.
  // Assert: chip reverts to queued.
  // Assert: no "cancelled" / "crash" / "recover" / "resume" copy anywhere.
});
