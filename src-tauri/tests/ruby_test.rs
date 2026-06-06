use lingq_upload_lib::core::text::strip_ruby;

#[test]
fn basic_kanji_ruby_strips_to_base() {
    let s = "<ruby>漢<rt>かん</rt></ruby>";
    assert_eq!(strip_ruby(s), "漢");
}

#[test]
fn rb_base_honoured() {
    let s = "<ruby><rb>漢字</rb><rt>かんじ</rt></ruby>";
    assert_eq!(strip_ruby(s), "漢字");
}

#[test]
fn rp_parens_stripped() {
    let s = "<ruby>漢<rp>(</rp><rt>かん</rt><rp>)</rp></ruby>";
    assert_eq!(strip_ruby(s), "漢");
}

#[test]
fn nested_ruby_flattens() {
    let s = "<ruby><ruby>漢<rt>かん</rt></ruby>字<rt>じ</rt></ruby>";
    assert_eq!(strip_ruby(s), "漢字");
}

#[test]
fn orphan_whitespace_between_cjk_collapses() {
    let s = "漢   字";
    assert_eq!(strip_ruby(s), "漢字");
}

#[test]
fn whitespace_outside_cjk_preserved() {
    let s = "hello world";
    assert_eq!(strip_ruby(s), "hello world");
}

#[test]
fn whitespace_between_cjk_and_ascii_preserved() {
    let s = "漢 hello";
    assert_eq!(strip_ruby(s), "漢 hello");
}

#[test]
fn malformed_ruby_no_rt_passes_through() {
    let s = "<ruby>x</ruby>";
    assert_eq!(strip_ruby(s), "x");
}

#[test]
fn paragraph_with_ruby_and_plain() {
    let s = "<p>今日は<ruby>晴<rt>は</rt></ruby>れだ。</p>";
    assert_eq!(strip_ruby(s), "<p>今日は晴れだ。</p>");
}

#[test]
fn multiple_ruby_in_sequence() {
    let s = "<ruby>桜<rt>さくら</rt></ruby>と<ruby>梅<rt>うめ</rt></ruby>";
    assert_eq!(strip_ruby(s), "桜と梅");
}

#[test]
fn empty_string() {
    assert_eq!(strip_ruby(""), "");
}

#[test]
fn no_ruby_unchanged() {
    let s = "<p>plain html</p>";
    assert_eq!(strip_ruby(s), "<p>plain html</p>");
}

#[test]
fn stray_lt_between_cjk_does_not_corrupt_utf8() {
    // No closing '>' after the '<' so the parser walks past the bare '<'.
    // Must not emit U+FFFD anywhere — bytes around the '<' are all valid UTF-8.
    let s = "漢 < 字";
    let out = strip_ruby(s);
    assert!(!out.contains('\u{FFFD}'), "got {out:?}");
    assert!(out.contains('漢'));
    assert!(out.contains('字'));
}

#[test]
fn stray_lt_adjacent_to_multibyte_does_not_corrupt_utf8() {
    let s = "漢<字";
    let out = strip_ruby(s);
    assert!(!out.contains('\u{FFFD}'));
    assert_eq!(out, "漢<字");
}
