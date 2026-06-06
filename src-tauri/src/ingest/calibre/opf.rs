use quick_xml::events::Event;
use quick_xml::Reader;
use thiserror::Error;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpfMetadata {
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<f32>,
    pub tags: Vec<String>,
    pub isbn13: Option<String>,
    pub calibre_uuid: Option<String>,
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
    in_creator_aut: bool,
    in_language: bool,
    in_subject: bool,
    in_id_isbn: bool,
    in_id_calibre: bool,
    out: OpfMetadata,
}

pub fn parse_opf(xml: &str) -> Result<OpfMetadata, OpfError> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut acc = Acc::default();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let qn = e.name();
                let local = local_name(qn.as_ref());
                match local {
                    "title" => acc.in_title = true,
                    "creator" => {
                        let role = attr(&e, b"role").or_else(|| attr(&e, b"opf:role"));
                        if role.as_deref() == Some("aut") || role.is_none() {
                            acc.in_creator_aut = true;
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
                            Some(s) if s.contains("calibre") => acc.in_id_calibre = true,
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
                let s = s.trim();
                if s.is_empty() {
                    continue;
                }
                if acc.in_title && acc.out.title.is_empty() {
                    acc.out.title = s.into();
                }
                if acc.in_creator_aut {
                    acc.out.authors.push(s.into());
                }
                if acc.in_language && acc.out.language.is_none() {
                    acc.out.language = Some(s.into());
                }
                if acc.in_subject {
                    acc.out.tags.push(s.into());
                }
                if acc.in_id_isbn && acc.out.isbn13.is_none() {
                    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit() || *c == 'x' || *c == 'X').collect();
                    if cleaned.len() == 13 {
                        acc.out.isbn13 = Some(cleaned);
                    }
                }
                if acc.in_id_calibre && acc.out.calibre_uuid.is_none() {
                    acc.out.calibre_uuid = Some(s.into());
                }
            }
            Ok(Event::End(e)) => {
                let qn = e.name();
                let local = local_name(qn.as_ref());
                match local {
                    "title" => acc.in_title = false,
                    "creator" => acc.in_creator_aut = false,
                    "language" => acc.in_language = false,
                    "subject" => acc.in_subject = false,
                    "identifier" => {
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
