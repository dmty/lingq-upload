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
    struct RestoreFile {
        path: std::path::PathBuf,
        contents: Option<Vec<u8>>,
    }

    impl Drop for RestoreFile {
        fn drop(&mut self) {
            if let Some(contents) = &self.contents {
                std::fs::write(&self.path, contents).expect("restore bindings");
            }
        }
    }

    let original_dir = std::env::current_dir().expect("current_dir");
    let sandbox = tempfile::tempdir().expect("tempdir");
    let arbitrary_dir = sandbox.path().join("cwd");
    std::fs::create_dir(&arbitrary_dir).expect("create arbitrary current_dir");
    let bindings_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/ipc/bindings.ts");
    let mut restore = RestoreFile {
        contents: Some(std::fs::read(&bindings_path).expect("read bindings backup")),
        path: bindings_path.clone(),
    };
    std::fs::write(&bindings_path, b"not generated").expect("write binding sentinel");

    std::env::set_current_dir(&arbitrary_dir).expect("set arbitrary current_dir");
    let result = lingq_upload_lib::export_bindings();
    std::env::set_current_dir(original_dir).expect("restore current_dir");

    result.expect("export_bindings from arbitrary current_dir");
    let exported = std::fs::read_to_string(bindings_path).expect("read exported bindings");
    assert!(exported.contains("AUTO-GENERATED"));
    assert!(exported.contains("async cmdListTranscribeProviders()"));
    restore.contents = None;
}
