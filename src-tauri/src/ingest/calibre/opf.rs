use quick_xml::events::Event;
use quick_xml::Reader;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpfMetadata {
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<f32>,
    pub tags: Vec<String>,
    pub isbn13: Option<String>,
    pub calibre_uuid: Option<Uuid>,
}

#[derive(Debug, Error)]
pub enum OpfError {
    #[error("xml: {0}")]
    Xml(String),
    #[error("missing required field: {0}")]
    Missing(&'static str),
}

#[derive(Default)]
struct Acc {
    in_title: bool,
    in_creator: bool,
    creator_is_author: bool,
    creator_buf: String,
    in_language: bool,
    in_subject: bool,
    in_id_isbn: bool,
    in_id_calibre: bool,
    calibre_uuid_raw: String,
    explicit_author_seen: bool,
    out: OpfMetadata,
}

/// Parse a Calibre `metadata.opf` into [`OpfMetadata`].
///
/// Two-pass design choices worth knowing:
/// - `<dc:creator>` text is buffered per element and only flushed on close.
///   Calibre sometimes splits author names across mixed-content children
///   (`<creator>John <span>Q.</span> Public</creator>`); the old per-event
///   flush produced ghost authors.
/// - A creator is treated as an author only when it carries an explicit
///   `role="aut"` (or `opf:role="aut"`). If *no* creator in the file has any
///   role attribute, we fall back to accepting all creators as authors —
///   older Calibre exports omit roles entirely.
/// - ISBN is captured only when the digit-filtered text is exactly 13 digits.
///   The `X` check-digit is ISBN-10 only and is rejected here.
/// - Calibre UUID is parsed into [`Uuid`] at parse time; invalid values are
///   dropped with a warning.
pub fn parse_opf(xml: &str) -> Result<OpfMetadata, OpfError> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut acc = Acc::default();
    let mut pending_authors: Vec<(bool, String)> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let qn = e.name();
                let local = local_name(qn.as_ref());
                match local {
                    "title" => acc.in_title = true,
                    "creator" => {
                        acc.in_creator = true;
                        acc.creator_buf.clear();
                        let role = attr(&e, b"role").or_else(|| attr(&e, b"opf:role"));
                        match role.as_deref() {
                            Some("aut") => {
                                acc.creator_is_author = true;
                                acc.explicit_author_seen = true;
                            }
                            Some(_) => acc.creator_is_author = false,
                            None => acc.creator_is_author = false,
                        }
                    }
                    "language" => acc.in_language = true,
                    "subject" => acc.in_subject = true,
                    "identifier" => {
                        let scheme = attr(&e, b"scheme")
                            .or_else(|| attr(&e, b"opf:scheme"))
                            .map(|s| s.to_lowercase());
                        match scheme.as_deref() {
                            Some("isbn") => acc.in_id_isbn = true,
                            Some(s) if s.contains("calibre") => {
                                acc.in_id_calibre = true;
                                acc.calibre_uuid_raw.clear();
                            }
                            _ => {}
                        }
                    }
                    "meta" => {
                        let name = attr(&e, b"name").unwrap_or_default();
                        let content = attr(&e, b"content").unwrap_or_default();
                        apply_meta(&mut acc.out, &name, &content);
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let qn = e.name();
                if local_name(qn.as_ref()) == "meta" {
                    let name = attr(&e, b"name").unwrap_or_default();
                    let content = attr(&e, b"content").unwrap_or_default();
                    apply_meta(&mut acc.out, &name, &content);
                }
            }
            Ok(Event::Text(t)) => {
                let s = t.unescape().map_err(|e| OpfError::Xml(e.to_string()))?;
                let s_trim = s.trim();
                if s_trim.is_empty() {
                    continue;
                }
                if acc.in_title && acc.out.title.is_empty() {
                    acc.out.title = s_trim.into();
                }
                if acc.in_creator {
                    if !acc.creator_buf.is_empty() {
                        acc.creator_buf.push(' ');
                    }
                    acc.creator_buf.push_str(s_trim);
                }
                if acc.in_language && acc.out.language.is_none() {
                    acc.out.language = Some(s_trim.into());
                }
                if acc.in_subject {
                    acc.out.tags.push(s_trim.into());
                }
                if acc.in_id_isbn && acc.out.isbn13.is_none() {
                    let digits: String = s_trim.chars().filter(|c| c.is_ascii_digit()).collect();
                    if digits.len() == 13 {
                        acc.out.isbn13 = Some(digits);
                    }
                }
                if acc.in_id_calibre {
                    acc.calibre_uuid_raw.push_str(s_trim);
                }
            }
            Ok(Event::End(e)) => {
                let qn = e.name();
                let local = local_name(qn.as_ref());
                match local {
                    "title" => acc.in_title = false,
                    "creator" => {
                        let name = acc.creator_buf.trim().to_string();
                        if !name.is_empty() {
                            pending_authors.push((acc.creator_is_author, name));
                        }
                        acc.in_creator = false;
                        acc.creator_is_author = false;
                        acc.creator_buf.clear();
                    }
                    "language" => acc.in_language = false,
                    "subject" => acc.in_subject = false,
                    "identifier" => {
                        if acc.in_id_calibre && acc.out.calibre_uuid.is_none() {
                            let raw = acc.calibre_uuid_raw.trim();
                            let cleaned = raw.strip_prefix("urn:uuid:").unwrap_or(raw);
                            match Uuid::parse_str(cleaned) {
                                Ok(u) => acc.out.calibre_uuid = Some(u),
                                Err(_) => {
                                    tracing::warn!(uuid = %raw, "invalid calibre uuid; dropping");
                                }
                            }
                            acc.calibre_uuid_raw.clear();
                        }
                        acc.in_id_isbn = false;
                        acc.in_id_calibre = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(OpfError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    // Author resolution: prefer explicitly-marked authors; fall back to all
    // creators when no role attribute was present anywhere in the file.
    if acc.explicit_author_seen {
        acc.out.authors = pending_authors
            .into_iter()
            .filter(|(is_aut, _)| *is_aut)
            .map(|(_, n)| n)
            .collect();
    } else {
        acc.out.authors = pending_authors.into_iter().map(|(_, n)| n).collect();
    }

    if acc.out.title.is_empty() {
        return Err(OpfError::Missing("title"));
    }
    Ok(acc.out)
}

fn local_name(qn: &[u8]) -> &str {
    let s = std::str::from_utf8(qn).unwrap_or("");
    match s.rfind(':') {
        Some(i) => &s[i + 1..],
        None => s,
    }
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    for a in e.attributes().flatten() {
        let k = a.key.as_ref();
        if k == key || k.rsplit(|&b| b == b':').next().unwrap_or(&[]) == key {
            return a.unescape_value().ok().map(|v| v.into_owned());
        }
    }
    None
}

fn apply_meta(out: &mut OpfMetadata, name: &str, content: &str) {
    match name {
        "calibre:series" => out.series = Some(content.to_string()),
        "calibre:series_index" => out.series_index = content.parse().ok(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_role_aut_is_author() {
        let xml = r#"<?xml version="1.0"?>
        <package xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
          <metadata>
            <dc:title>T</dc:title>
            <dc:creator opf:role="aut">Real Author</dc:creator>
            <dc:creator opf:role="edt">Editor Person</dc:creator>
            <dc:creator opf:role="trl">Translator</dc:creator>
          </metadata>
        </package>"#;
        let m = parse_opf(xml).unwrap();
        assert_eq!(m.authors, vec!["Real Author"]);
    }

    #[test]
    fn no_role_anywhere_accepts_all_creators() {
        let xml = r#"<?xml version="1.0"?>
        <package xmlns:dc="http://purl.org/dc/elements/1.1/">
          <metadata>
            <dc:title>T</dc:title>
            <dc:creator>Author A</dc:creator>
            <dc:creator>Author B</dc:creator>
          </metadata>
        </package>"#;
        let m = parse_opf(xml).unwrap();
        assert_eq!(m.authors, vec!["Author A", "Author B"]);
    }

    #[test]
    fn mixed_content_creator_does_not_split_into_ghosts() {
        // Real-world: name interleaved with markup inside <creator>.
        let xml = r#"<?xml version="1.0"?>
        <package xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
          <metadata>
            <dc:title>T</dc:title>
            <dc:creator opf:role="aut">John <span>Q.</span> Public</dc:creator>
          </metadata>
        </package>"#;
        let m = parse_opf(xml).unwrap();
        assert_eq!(m.authors.len(), 1);
        assert!(m.authors[0].contains("John"));
        assert!(m.authors[0].contains("Public"));
    }

    #[test]
    fn isbn_rejects_x_check_digit() {
        let xml = r#"<?xml version="1.0"?>
        <package xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
          <metadata>
            <dc:title>T</dc:title>
            <dc:creator opf:role="aut">A</dc:creator>
            <dc:identifier opf:scheme="ISBN">978410100101X</dc:identifier>
          </metadata>
        </package>"#;
        let m = parse_opf(xml).unwrap();
        assert!(m.isbn13.is_none());
    }

    #[test]
    fn isbn_accepts_13_digits_with_hyphens() {
        let xml = r#"<?xml version="1.0"?>
        <package xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
          <metadata>
            <dc:title>T</dc:title>
            <dc:creator opf:role="aut">A</dc:creator>
            <dc:identifier opf:scheme="ISBN">978-4-10-100101-2</dc:identifier>
          </metadata>
        </package>"#;
        let m = parse_opf(xml).unwrap();
        assert_eq!(m.isbn13.as_deref(), Some("9784101001012"));
    }

    #[test]
    fn calibre_uuid_parsed_into_uuid_type() {
        let xml = r#"<?xml version="1.0"?>
        <package xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
          <metadata>
            <dc:title>T</dc:title>
            <dc:creator opf:role="aut">A</dc:creator>
            <dc:identifier opf:scheme="calibre">aaaa1111-bbbb-2222-cccc-333344445555</dc:identifier>
          </metadata>
        </package>"#;
        let m = parse_opf(xml).unwrap();
        let u = m.calibre_uuid.unwrap();
        assert_eq!(u.to_string(), "aaaa1111-bbbb-2222-cccc-333344445555");
    }

    #[test]
    fn calibre_uuid_invalid_dropped() {
        let xml = r#"<?xml version="1.0"?>
        <package xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
          <metadata>
            <dc:title>T</dc:title>
            <dc:creator opf:role="aut">A</dc:creator>
            <dc:identifier opf:scheme="calibre">not-a-uuid</dc:identifier>
          </metadata>
        </package>"#;
        let m = parse_opf(xml).unwrap();
        assert!(m.calibre_uuid.is_none());
    }

    #[test]
    fn calibre_uuid_with_urn_prefix() {
        let xml = r#"<?xml version="1.0"?>
        <package xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
          <metadata>
            <dc:title>T</dc:title>
            <dc:creator opf:role="aut">A</dc:creator>
            <dc:identifier opf:scheme="calibre">urn:uuid:aaaa1111-bbbb-2222-cccc-333344445555</dc:identifier>
          </metadata>
        </package>"#;
        let m = parse_opf(xml).unwrap();
        assert!(m.calibre_uuid.is_some());
    }
}
