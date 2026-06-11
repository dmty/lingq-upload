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
        (
            "OEBPS/ch1.xhtml",
            body("Chapter One", "Opening prose.").into_bytes(),
        ),
        (
            "OEBPS/ch2.xhtml",
            body("Chapter Two", "Middle prose.").into_bytes(),
        ),
        (
            "OEBPS/ch3.xhtml",
            body("Chapter Three", "Closing prose.").into_bytes(),
        ),
    ];
    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
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

/// Most trade EPUBs ship an `toc.ncx` whose `<navLabel><text>` carries the
/// canonical chapter title — and many style chapter titles as `<p>` blocks
/// (esp. Japanese vertical layout). With no NCX path, those books degrade to
/// "Chapter 1", "Chapter 2"… Pinning the NCX path here keeps the regression
/// reproducible.
#[test]
fn kindle_picks_chapter_titles_from_ncx_when_no_h1() {
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata/>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx">
    <itemref idref="c1"/>
    <itemref idref="c2"/>
  </spine>
</package>"#;
    let ncx = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <navMap>
    <navPoint id="n1" playOrder="1">
      <navLabel><text>理科教室の黒い影</text></navLabel>
      <content src="ch1.xhtml"/>
    </navPoint>
    <navPoint id="n2" playOrder="2">
      <navLabel><text>たそがれの校舎</text></navLabel>
      <content src="ch2.xhtml"/>
    </navPoint>
  </navMap>
</ncx>"#;
    // Bodies have no <h1>/<h2>/<h3> — chapter title is styled as <p>, the
    // common Japanese trade-EPUB pattern.
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>book title</title></head>
<body><p class="chap-tit">理科教室の黒い影</p><p>放課後の校舎は静かで…</p></body></html>"#;
    let ch2 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>book title</title></head>
<body><p class="chap-tit">たそがれの校舎</p><p>夕闇が近づき…</p></body></html>"#;

    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in [
            ("mimetype", b"application/epub+zip" as &[u8]),
            ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/toc.ncx", ncx.as_bytes()),
            ("OEBPS/ch1.xhtml", ch1.as_bytes()),
            ("OEBPS/ch2.xhtml", ch2.as_bytes()),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    let chapters = parse_epub_with_strategy(&buf, HeadingStrategy::Kindle).expect("parse");
    assert_eq!(chapters.len(), 2);
    assert_eq!(chapters[0].title, "理科教室の黒い影");
    assert_eq!(chapters[1].title, "たそがれの校舎");
}

/// Japanese trade EPUBs commonly split one logical chapter across several
/// spine files but only list the chapter anchor (the first split) in the
/// NCX. The NCX is the canonical chapter list — so the parser collapses
/// consecutive sub-pages into the NCX-anchored chapter, concatenating their
/// body text. Leading entries before the first NCX hit stay separate
/// (cover / copyright / TOC pages have no parent to inherit from).
#[test]
fn kindle_collapses_split_chapter_subpages_into_toc_anchored_groups() {
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata/>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="fm"  href="fm.xhtml"  media-type="application/xhtml+xml"/>
    <item id="p1"  href="p1.xhtml"  media-type="application/xhtml+xml"/>
    <item id="p2"  href="p2.xhtml"  media-type="application/xhtml+xml"/>
    <item id="p3"  href="p3.xhtml"  media-type="application/xhtml+xml"/>
    <item id="c1"  href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c1b" href="ch1b.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx">
    <itemref idref="fm"/>
    <itemref idref="p1"/>
    <itemref idref="p2"/>
    <itemref idref="p3"/>
    <itemref idref="c1"/>
    <itemref idref="c1b"/>
  </spine>
</package>"#;
    // NCX names the prologue (starts at p1) and chapter one (starts at c1).
    // p2 and p3 are split continuations of プロローグ; c1b is a continuation
    // of 一章. fm is leading front matter — no inheritance, falls back.
    let ncx = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <navMap>
    <navPoint><navLabel><text>プロローグ</text></navLabel><content src="p1.xhtml"/></navPoint>
    <navPoint><navLabel><text>一章 同期が来たりて</text></navLabel><content src="ch1.xhtml"/></navPoint>
  </navMap>
</ncx>"#;
    // Bodies with no `<head>` keep this test focused on the fold step; the
    // Kindle html-stripper leaves head text in place which is a separate
    // concern.
    let body = |p: &str| -> String {
        format!(
            r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>{p}</p></body></html>"#
        )
    };
    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body_bytes) in [
            ("mimetype", b"application/epub+zip".to_vec()),
            ("META-INF/container.xml", CONTAINER_XML.as_bytes().to_vec()),
            ("OEBPS/content.opf", opf.as_bytes().to_vec()),
            ("OEBPS/toc.ncx", ncx.as_bytes().to_vec()),
            ("OEBPS/fm.xhtml", body("front matter").into_bytes()),
            ("OEBPS/p1.xhtml", body("prologue start").into_bytes()),
            ("OEBPS/p2.xhtml", body("prologue continues").into_bytes()),
            (
                "OEBPS/p3.xhtml",
                body("prologue continues more").into_bytes(),
            ),
            ("OEBPS/ch1.xhtml", body("chapter one start").into_bytes()),
            (
                "OEBPS/ch1b.xhtml",
                body("chapter one continues").into_bytes(),
            ),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(&body_bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    let chapters = parse_epub_with_strategy(&buf, HeadingStrategy::Kindle).expect("parse");
    let titles: Vec<&str> = chapters.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(
        titles,
        vec![
            "Chapter 1",           // fm: leading, no parent to merge into
            "プロローグ",          // p1 + p2 + p3 collapsed
            "一章 同期が来たりて", // c1 + c1b collapsed
        ],
        "spine sub-pages must collapse into their NCX-anchored chapter",
    );
    // Merged body uses "\n\n" as the spine-sub-page separator. Pinned so a
    // future change to the separator surfaces in this test rather than
    // silently shifting matcher behavior.
    let prologue = chapters
        .iter()
        .find(|c| c.title == "プロローグ")
        .expect("prologue present");
    assert_eq!(
        prologue.body,
        "prologue start\n\nprologue continues\n\nprologue continues more",
    );
}

/// No NCX, no nav, no `<h1>` — every spine entry must stand alone with the
/// `"Chapter N"` fallback. Confirms the fold step is a no-op when there are
/// no TOC anchors, matching the existing `missing_nav_falls_back_to_html_title`
/// behavior on the Kobo side.
#[test]
fn kindle_with_no_toc_keeps_every_spine_entry_separate() {
    let opf = r#"<?xml version="1.0"?>
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
    let body_no_heading = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>just prose</p></body></html>"#;
    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in [
            ("mimetype", b"application/epub+zip" as &[u8]),
            ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/ch1.xhtml", body_no_heading.as_bytes()),
            ("OEBPS/ch2.xhtml", body_no_heading.as_bytes()),
            ("OEBPS/ch3.xhtml", body_no_heading.as_bytes()),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }
    let chapters = parse_epub_with_strategy(&buf, HeadingStrategy::Kindle).expect("parse");
    assert_eq!(
        chapters
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Chapter 1", "Chapter 2", "Chapter 3"],
        "with no TOC and no heading, fold is a no-op and every spine entry stays separate",
    );
}

/// EPUB3 nav.xhtml in a Kindle/Generic book — same priority as NCX, just a
/// different file format.
#[test]
fn kindle_picks_chapter_titles_from_nav_when_no_h1() {
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata/>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
    let nav = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body><nav epub:type="toc"><ol><li><a href="ch1.xhtml">時をかける少女</a></li></ol></nav></body></html>"#;
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body><p>本文の本文…</p></body></html>"#;
    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in [
            ("mimetype", b"application/epub+zip" as &[u8]),
            ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/nav.xhtml", nav.as_bytes()),
            ("OEBPS/ch1.xhtml", ch1.as_bytes()),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }
    let chapters = parse_epub_with_strategy(&buf, HeadingStrategy::Kindle).expect("parse");
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0].title, "時をかける少女");
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
