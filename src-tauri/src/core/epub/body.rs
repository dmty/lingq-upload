//! Shared chapter-body cleaning. Both heading strategies funnel each spine
//! file's raw XHTML through [`clean_chapter_body`] before emitting it as a
//! `Chapter::body` blob: tags out, ruby annotations folded, whitespace
//! collapsed, `<style>` / `<script>` payloads dropped.

use super::find_case_insensitive;
use crate::core::text::{next_char_at, strip_ruby};

pub(crate) fn clean_chapter_body(html: &str) -> String {
    let stripped = strip_ruby(html);
    let text = strip_html_tags(&stripped);
    collapse_whitespace(&text)
}

fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // <style>/<script> payloads are not prose — drop them whole.
            if let Some(end) = raw_element_end(html, i) {
                i = end;
                continue;
            }
            match bytes[i + 1..].iter().position(|&b| b == b'>') {
                Some(p) => i = i + 1 + p + 1,
                None => {
                    // '<' is always 1 byte in UTF-8 — emitting it as-is and
                    // stepping by one cannot land mid-codepoint.
                    out.push('<');
                    i += 1;
                }
            }
            continue;
        }
        let ch = next_char_at(bytes, i);
        out.push(ch);
        i += ch.len_utf8().max(1);
    }
    decode_basic_entities(&out)
}

/// If `html[start..]` opens a `<style>` or `<script>` element, return the
/// byte offset just past its closing tag (or EOF when unterminated) so the
/// caller skips content and tags in one hop.
fn raw_element_end(html: &str, start: usize) -> Option<usize> {
    let rest = &html[start + 1..];
    let name = ["style", "script"].into_iter().find(|n| {
        rest.len() > n.len()
            && rest.as_bytes()[..n.len()].eq_ignore_ascii_case(n.as_bytes())
            && matches!(
                rest.as_bytes()[n.len()],
                b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n'
            )
    })?;
    if let Some(gt) = rest.find('>') {
        if rest.as_bytes()[..gt].ends_with(b"/") {
            return Some(start + 1 + gt + 1);
        }
    }
    let close = format!("</{name}");
    let close_rel = match find_case_insensitive(rest, &close) {
        Some(p) => p,
        None => return Some(html.len()),
    };
    let after_close = start + 1 + close_rel;
    match html[after_close..].find('>') {
        Some(p) => Some(after_close + p + 1),
        None => Some(html.len()),
    }
}

// `&amp;` must decode last or `&amp;lt;` double-decodes to `<`.
fn decode_basic_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#x3000;", "\u{3000}")
        .replace("&amp;", "&")
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_blank = false;
    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_blank {
                out.push('\n');
                prev_blank = true;
            }
        } else {
            out.push_str(trimmed);
            out.push('\n');
            prev_blank = false;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_basic_entities_amp_decodes_last() {
        assert_eq!(decode_basic_entities("&amp;lt;"), "&lt;");
        assert_eq!(decode_basic_entities("&amp;amp;"), "&amp;");
        assert_eq!(
            decode_basic_entities("&lt;b&gt; &amp; &quot;q&quot;"),
            "<b> & \"q\""
        );
    }

    #[test]
    fn strip_html_tags_drops_style_and_script_content() {
        let html = "<head><style>.a { color: red }</style></head>\
<body><p>Keep</p><script>var x = 1;</script><p>Also keep</p></body>";
        let out = strip_html_tags(html);
        assert!(!out.contains("color"), "css leaked: {out}");
        assert!(!out.contains("var x"), "js leaked: {out}");
        assert!(out.contains("Keep") && out.contains("Also keep"));
    }

    #[test]
    fn strip_html_tags_self_closing_style_keeps_following_text() {
        let out = strip_html_tags("<style type=\"text/css\"/><p>Body text</p>");
        assert_eq!(out.trim(), "Body text");
    }
}
