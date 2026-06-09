//! Regenerates `src/lib/ipc/bindings.ts` from the tauri-specta Builder.
//!
//! Wired as a test so `cargo test` (run by both devs and CI) keeps the
//! checked-in TypeScript bindings in sync with Rust signatures. CI then
//! does `git diff --exit-code` to catch drift.
//!
//! Skipped on Windows because the test binary transitively links the
//! tauri-runtime-wry WebView2 chain and fails to launch with
//! STATUS_ENTRYPOINT_NOT_FOUND on the hosted runner image. CI's bindings drift
//! gate is Linux-only anyway, so Windows running this test adds no signal.
#![cfg(not(windows))]

#[test]
fn bindings_export_in_sync() {
    lingq_upload_lib::export_bindings().expect("export_bindings");
}
