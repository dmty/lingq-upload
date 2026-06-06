use std::path::PathBuf;

fn main() {
    emit_bundle_identifier();
    tauri_build::build();
}

fn emit_bundle_identifier() {
    let conf_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    println!("cargo:rerun-if-changed={}", conf_path.display());

    let raw = std::fs::read_to_string(&conf_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", conf_path.display()));
    let value: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("parse {}: {e}", conf_path.display()));
    let identifier = value
        .get("identifier")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("`identifier` missing or non-string in tauri.conf.json"));
    println!("cargo:rustc-env=LINGQ_BUNDLE_ID={identifier}");
}
