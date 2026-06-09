//! Pin the Kindle path's chapter shape after the deterministic-id change.
//!
//! The fixture is built in-test (zero binary churn) and matches the
//! per-strategy contract: same EPUB bytes → identical chapter ids and
//! titles. If a future tweak to the Kindle parser drifts either, the
//! snapshot review surfaces it.

use std::io::{Cursor, Write};

use insta::assert_json_snapshot;
use lingq_upload_lib::core::epub::{parse_epub_with_strategy, ChapterId, HeadingStrategy};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const CONTAINER_XML: &str = r#"<?xml version="1.0"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

const OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata/>
  <manifest>
    <item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
    <item id="c3" href="ch3.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="c2"/>
    <itemref idref="c3"/>
  </spine>
</package>"#;

fn body(heading: &str, p: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>x</title></head>
<body><div class="calibre"><h1>{heading}</h1><p>{p}</p></div></body></html>"#
    )
}

fn build_kindle_epub() -> Vec<u8> {
    let entries: Vec<(&'static str, Vec<u8>)> = vec![
        ("mimetype", b"application/epub+zip".to_vec()),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes().to_vec()),
        ("OEBPS/content.opf", OPF.as_bytes().to_vec()),
        ("OEBPS/ch1.xhtml", body("Chapter One", "Opening prose.").into_bytes()),
        ("OEBPS/ch2.xhtml", body("Chapter Two", "Middle prose.").into_bytes()),
        ("OEBPS/ch3.xhtml", body("Chapter Three", "Closing prose.").into_bytes()),
    ];
    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in &entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

#[test]
fn kindle_chapters_pin_snapshot() {
    let bytes = build_kindle_epub();
    let chapters = parse_epub_with_strategy(&bytes, HeadingStrategy::Kindle).expect("parse kindle");
    let pinned: Vec<_> = chapters
        .iter()
        .map(|c| {
            serde_json::json!({
                "order": c.order,
                "title": c.title,
                "id": c.id.0,
                "kind": c.kind,
            })
        })
        .collect();
    assert_json_snapshot!("kindle_synth_chapters", pinned);
}

#[test]
fn kindle_chapter_ids_are_deterministic() {
    let bytes = build_kindle_epub();
    let a = parse_epub_with_strategy(&bytes, HeadingStrategy::Kindle).expect("a");
    let b = parse_epub_with_strategy(&bytes, HeadingStrategy::Kindle).expect("b");
    let ids_a: Vec<ChapterId> = a.iter().map(|c| c.id.clone()).collect();
    let ids_b: Vec<ChapterId> = b.iter().map(|c| c.id.clone()).collect();
    assert_eq!(ids_a, ids_b);
    for c in &a {
        assert!(c.id.0.starts_with("kindle:"), "wrong prefix: {}", c.id.0);
    }
}
