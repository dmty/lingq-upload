/// Strip `<ruby>` furigana annotations from HTML, preserving base text.
///
/// Kindle JP EPUBs wrap kanji like `<ruby>漢<rt>かん</rt></ruby>` which pollutes
/// matcher hashes and renders as `漢(かん)` in LingQ lesson text.
///
/// Rules:
/// - `<ruby>BASE<rt>READING</rt></ruby>` → `BASE`.
/// - `<rb>BASE</rb>` honoured.
/// - `<rp>(…)</rp>` parens dropped.
/// - Nested `<ruby>` flattened.
/// - Orphan whitespace between two CJK Unified Ideographs collapsed.
/// - Malformed `<ruby>x</ruby>` with no `<rt>` passes the inner text through.
pub fn strip_ruby(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut i = 0;
    let len = bytes.len();
    let mut inside_rt = 0u32;
    let mut inside_rp = 0u32;

    while i < len {
        if bytes[i] == b'<' {
            let tag_end = match find_byte(bytes, b'>', i + 1) {
                Some(e) => e,
                None => {
                    // Stray '<' with no closing '>': emit it and step by one
                    // ASCII byte. '<' is a single-byte UTF-8 codepoint so this
                    // never lands mid-sequence.
                    out.push('<');
                    i += 1;
                    continue;
                }
            };
            let raw = &html[i + 1..tag_end];
            let (name, closing) = parse_tag(raw);
            let n = name.to_ascii_lowercase();
            match (n.as_str(), closing) {
                ("ruby" | "rb", _) => {}
                ("rt", false) => inside_rt += 1,
                ("rt", true) => inside_rt = inside_rt.saturating_sub(1),
                ("rp", false) => inside_rp += 1,
                ("rp", true) => inside_rp = inside_rp.saturating_sub(1),
                _ => {
                    if inside_rt == 0 && inside_rp == 0 {
                        out.push_str(&html[i..=tag_end]);
                    }
                }
            }
            i = tag_end + 1;
            continue;
        }
        if inside_rt == 0 && inside_rp == 0 {
            let ch_start = i;
            let ch = next_char(bytes, i);
            let ch_len = ch.len_utf8();
            out.push(ch);
            i = ch_start + ch_len;
        } else {
            let ch_start = i;
            let ch = next_char(bytes, i);
            i = ch_start + ch.len_utf8();
        }
    }

    collapse_inter_cjk_whitespace(&out)
}

fn find_byte(bytes: &[u8], target: u8, start: usize) -> Option<usize> {
    bytes[start..]
        .iter()
        .position(|&b| b == target)
        .map(|p| p + start)
}

fn parse_tag(raw: &str) -> (&str, bool) {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix('/') {
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '/')
            .unwrap_or(rest.len());
        (&rest[..name_end], true)
    } else {
        let cleaned = raw.trim_end_matches('/');
        let name_end = cleaned
            .find(|c: char| c.is_whitespace())
            .unwrap_or(cleaned.len());
        (&cleaned[..name_end], false)
    }
}

fn next_char(bytes: &[u8], i: usize) -> char {
    std::str::from_utf8(&bytes[i..])
        .ok()
        .and_then(|s| s.chars().next())
        .unwrap_or('\u{FFFD}')
}

fn collapse_inter_cjk_whitespace(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if (c == ' ' || c == '\t') && i > 0 && is_cjk_ideograph(chars[i - 1]) {
            let mut j = i;
            while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                j += 1;
            }
            if j < chars.len() && is_cjk_ideograph(chars[j]) {
                i = j;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn is_cjk_ideograph(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF |    // CJK Unified Ideographs
        0x3400..=0x4DBF |    // Extension A
        0x20000..=0x2A6DF |  // Extension B
        0xF900..=0xFAFF      // Compatibility
    )
}
