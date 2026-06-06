//! Regenerates `src/lib/ipc/bindings.ts` from the tauri-specta Builder.
//!
//! Wired as a test so `cargo test` (run by both devs and CI) keeps the
//! checked-in TypeScript bindings in sync with Rust signatures. CI then
//! does `git diff --exit-code` to catch drift.

#[test]
fn bindings_export_in_sync() {
    lingq_upload_lib::export_bindings().expect("export_bindings");
}
