//! Text-encoding round-trip tests for `core::text::read_text_for_upload`.
//! Catches BOM injection, NFC-vs-NFD divergence and ZWSP corruption.

use std::io::Write;

#[path = "../src/core/text.rs"]
mod text;

fn write_tmp(name: &str, bytes: &[u8]) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(name)
        .tempfile()
        .expect("tempfile");
    f.write_all(bytes).expect("write");
    f.flush().expect("flush");
    f
}

#[test]
fn strips_utf8_bom() {
    let payload = "\u{feff}カフカ";
    let f = write_tmp(".txt", payload.as_bytes());
    let out = text::read_text_for_upload(f.path()).expect("read");
    assert_eq!(out, "カフカ");
}

#[test]
fn folds_nfd_to_nfc() {
    // U+30AB KATAKANA KA + U+3099 combining voicing == precomposed U+30AC GA.
    let payload = "\u{30AB}\u{3099}";
    let f = write_tmp(".txt", payload.as_bytes());
    let out = text::read_text_for_upload(f.path()).expect("read");
    assert_eq!(out, "\u{30AC}");
}

#[test]
fn zwsp_passes_through_unchanged() {
    let payload = "あ\u{200B}い";
    let f = write_tmp(".txt", payload.as_bytes());
    let out = text::read_text_for_upload(f.path()).expect("read");
    assert_eq!(out, payload);
}
