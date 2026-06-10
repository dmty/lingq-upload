//! Kobo EPUB parser.
//!
//! No real `運命を創る` EPUB is checked in (license + binary churn); the
//! suite synthesises a Kobo-flavoured EPUB3 in-memory with a `nav.xhtml`
//! TOC, koboSpan markers and front/back-matter chapters. Title and chapter
//! counts are pinned via `insta` snapshots so a parser regression surfaces
//! on the next snapshot review.

use std::io::{Cursor, Write};

use insta::assert_json_snapshot;
use lingq_upload_lib::core::epub::{
    autodetect_vendor_bytes, parse_epub_bytes, parse_epub_with_strategy, ChapterId, ChapterKind,
    EpubError, EpubVendor, HeadingStrategy,
};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const CONTAINER_XML: &str = r#"<?xml version="1.0"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

fn opf_with_nav() -> &'static str {
    r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata/>
  <manifest>
    <item id="nav"   href="nav.xhtml"   media-type="application/xhtml+xml" properties="nav"/>
    <item id="cover" href="cover.xhtml" media-type="application/xhtml+xml"/>
    <item id="c1"    href="ch1.xhtml"   media-type="application/xhtml+xml"/>
    <item id="c2"    href="ch2.xhtml"   media-type="application/xhtml+xml"/>
    <item id="c3"    href="ch3.xhtml"   media-type="application/xhtml+xml"/>
    <item id="ata"   href="ata.xhtml"   media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="cover"/>
    <itemref idref="c1"/>
    <itemref idref="c2"/>
    <itemref idref="c3"/>
    <itemref idref="ata"/>
  </spine>
</package>"#
}

fn nav_xhtml() -> &'static str {
    r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>TOC</title></head>
<body>
  <nav epub:type="toc">
    <ol>
      <li><a href="cover.xhtml">Cover</a></li>
      <li><a href="ch1.xhtml">The Beginning</a></li>
      <li><a href="ch2.xhtml">The Middle</a></li>
      <li><a href="ch3.xhtml">The End</a></li>
      <li><a href="ata.xhtml">About the Author</a></li>
    </ol>
  </nav>
</body></html>"#
}

fn kobo_body(text: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>page</title></head>
<body>
  <div class="font-160per">
    <p><span class="koboSpan" id="kobo.1.1">{text}</span></p>
    <p><span class="koboSpan" id="kobo.1.2">Another sentence.</span></p>
    <p><span class="koboSpan" id="kobo.1.3">A third sentence.</span></p>
  </div>
</body></html>"#
    )
}

fn build_kobo_epub() -> Vec<u8> {
    let entries: Vec<(&'static str, Vec<u8>)> = vec![
        ("mimetype", b"application/epub+zip".to_vec()),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes().to_vec()),
        ("OEBPS/content.opf", opf_with_nav().as_bytes().to_vec()),
        ("OEBPS/nav.xhtml", nav_xhtml().as_bytes().to_vec()),
        ("OEBPS/cover.xhtml", kobo_body("Cover image alt text.").into_bytes()),
        ("OEBPS/ch1.xhtml", kobo_body("Opening paragraph.").into_bytes()),
        ("OEBPS/ch2.xhtml", kobo_body("Middle paragraph.").into_bytes()),
        ("OEBPS/ch3.xhtml", kobo_body("Closing paragraph.").into_bytes()),
        ("OEBPS/ata.xhtml", kobo_body("Author bio text.").into_bytes()),
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
fn parses_chapter_count_and_titles_from_nav() {
    let bytes = build_kobo_epub();
    let chapters = parse_epub_with_strategy(&bytes, HeadingStrategy::Kobo).expect("parse kobo");
    let pinned: Vec<_> = chapters
        .iter()
        .map(|c| {
            serde_json::json!({
                "order": c.order,
                "title": c.title,
                "kind": c.kind,
                "id": c.id.0,
            })
        })
        .collect();
    assert_json_snapshot!("kobo_synth_chapters", pinned);
}

#[test]
fn detected_kobo_routes_through_kobo_strategy() {
    let bytes = build_kobo_epub();
    let det = autodetect_vendor_bytes(&bytes).expect("detect");
    assert_eq!(det.vendor, EpubVendor::Kobo);

    let chapters = parse_epub_with_strategy(&bytes, HeadingStrategy::Kobo).expect("parse");
    assert_eq!(chapters.len(), 5);
    assert_eq!(chapters[0].title, "Cover");
    assert_eq!(chapters[1].title, "The Beginning");
    assert_eq!(chapters[4].title, "About the Author");

    // Autodetect routing must reach the same output as the pinned strategy —
    // i.e. parse_epub_bytes actually goes through detection, not a default.
    let auto = parse_epub_bytes(&bytes).expect("autodetect parse");
    assert_eq!(auto, chapters, "autodetect did not route to the Kobo strategy");
}

#[test]
fn chapter_ids_are_deterministic_across_calls() {
    let bytes = build_kobo_epub();
    let a = parse_epub_with_strategy(&bytes, HeadingStrategy::Kobo).expect("a");
    let b = parse_epub_with_strategy(&bytes, HeadingStrategy::Kobo).expect("b");
    let ids_a: Vec<ChapterId> = a.iter().map(|c| c.id.clone()).collect();
    let ids_b: Vec<ChapterId> = b.iter().map(|c| c.id.clone()).collect();
    assert_eq!(ids_a, ids_b, "kobo chapter ids must be deterministic");
    // Form: kobo:{16-hex}. Anchor is the spine href + normalised title;
    // no positional index in the surface form so dropping an empty chapter
    // does not shift later ids.
    for c in &a {
        let id = &c.id.0;
        assert!(id.starts_with("kobo:"), "wrong prefix: {id}");
        let parts: Vec<&str> = id.splitn(2, ':').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].len(), 16, "hash16 expected, got {id}");
        assert!(
            parts[1].chars().all(|c| c.is_ascii_hexdigit()),
            "non-hex chars in {id}"
        );
    }

    // Pin one full id string so a hash drift surfaces immediately.
    // Cover chapter: spine href "cover.xhtml", title "Cover" from nav.
    let cover_id = a
        .iter()
        .find(|c| c.title == "Cover")
        .map(|c| c.id.0.clone())
        .expect("cover chapter present");
    assert_eq!(
        cover_id,
        ChapterId::from_chapter_parts("kobo", "cover.xhtml", "Cover").0,
        "pinned cover id mismatch — hash anchor drifted",
    );
}

#[test]
fn chapter_ids_stable_when_empty_chapter_dropped() {
    // Same EPUB twice: once with an empty middle chapter that gets filtered
    // out, once where that middle is non-empty. Surviving chapters must keep
    // their ids — dropping an empty must not shift later hash anchors.
    let empty_body = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title></title></head>
<body><p>   </p></body></html>"#;
    let full_body = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>page</title></head>
<body><p><span class="koboSpan">Middle has text.</span></p></body></html>"#;

    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata/>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1"  href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2"  href="ch2.xhtml" media-type="application/xhtml+xml"/>
    <item id="c3"  href="ch3.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="c2"/>
    <itemref idref="c3"/>
  </spine>
</package>"#;
    let nav = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body>
  <nav epub:type="toc">
    <ol>
      <li><a href="ch1.xhtml">First</a></li>
      <li><a href="ch2.xhtml">Middle</a></li>
      <li><a href="ch3.xhtml">Last</a></li>
    </ol>
  </nav>
</body></html>"#;
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>page</title></head>
<body><p><span class="koboSpan">First text.</span></p></body></html>"#;
    let ch3 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>page</title></head>
<body><p><span class="koboSpan">Last text.</span></p></body></html>"#;

    let build = |middle: &str| -> Vec<u8> {
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
                ("OEBPS/ch2.xhtml", middle.as_bytes()),
                ("OEBPS/ch3.xhtml", ch3.as_bytes()),
            ] {
                zip.start_file(name, opts).unwrap();
                zip.write_all(body).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    };

    let with_middle = build(full_body);
    let without_middle = build(empty_body);
    let a = parse_epub_with_strategy(&with_middle, HeadingStrategy::Kobo).expect("a");
    let b = parse_epub_with_strategy(&without_middle, HeadingStrategy::Kobo).expect("b");
    assert_eq!(a.len(), 3);
    assert_eq!(b.len(), 2, "empty middle filtered");

    // First and Last keep the same ids regardless of whether Middle survived.
    let id = |chs: &[lingq_upload_lib::core::epub::Chapter], t: &str| -> String {
        chs.iter().find(|c| c.title == t).unwrap().id.0.clone()
    };
    assert_eq!(id(&a, "First"), id(&b, "First"));
    assert_eq!(id(&a, "Last"), id(&b, "Last"));
}

#[test]
fn front_and_back_matter_classified_by_title() {
    let bytes = build_kobo_epub();
    let chapters = parse_epub_with_strategy(&bytes, HeadingStrategy::Kobo).expect("parse");

    let cover = chapters.iter().find(|c| c.title == "Cover").expect("cover");
    assert_eq!(cover.kind, ChapterKind::FrontMatter);

    let ata = chapters
        .iter()
        .find(|c| c.title == "About the Author")
        .expect("ata");
    assert_eq!(ata.kind, ChapterKind::BackMatter);

    let middle = chapters
        .iter()
        .find(|c| c.title == "The Middle")
        .expect("middle");
    assert_eq!(middle.kind, ChapterKind::Body);
}

#[test]
fn missing_nav_falls_back_to_html_title() {
    // Same OPF but without the nav item or nav.xhtml; chapter titles come
    // from each spine file's `<title>` element.
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata/>
  <manifest>
    <item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="c2"/>
  </spine>
</package>"#;
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Prologue</title></head>
<body><p><span class="koboSpan">A.</span></p>
<p><span class="koboSpan">B.</span></p>
<p><span class="koboSpan">C.</span></p></body></html>"#;
    let ch2 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter Two</title></head>
<body><p><span class="koboSpan">X.</span></p>
<p><span class="koboSpan">Y.</span></p>
<p><span class="koboSpan">Z.</span></p></body></html>"#;
    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in [
            ("mimetype", b"application/epub+zip" as &[u8]),
            ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/ch1.xhtml", ch1.as_bytes()),
            ("OEBPS/ch2.xhtml", ch2.as_bytes()),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    let chapters = parse_epub_with_strategy(&buf, HeadingStrategy::Kobo).expect("parse no-nav");
    assert_eq!(chapters.len(), 2);
    assert_eq!(chapters[0].title, "Prologue");
    assert_eq!(chapters[1].title, "Chapter Two");
    assert_eq!(chapters[0].kind, ChapterKind::FrontMatter);
}

#[test]
fn all_undecodable_spine_returns_error_not_empty_ok() {
    // Both spine files are Shift-JIS — undecodable as utf-8/utf-16. A book
    // whose every chapter is skipped must surface an error, not Ok(vec![]).
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata/>
  <manifest>
    <item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="c2"/>
  </spine>
</package>"#;
    // "日本語" in Shift-JIS, wrapped in ASCII markup: invalid utf-8, no BOM.
    let sjis_body: &[u8] = b"<html><body><p>\x93\xfa\x96\x7b\x8c\xea</p></body></html>";
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
            ("OEBPS/ch1.xhtml", sjis_body),
            ("OEBPS/ch2.xhtml", sjis_body),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    let err = parse_epub_with_strategy(&buf, HeadingStrategy::Kobo)
        .expect_err("all-undecodable spine must error");
    match err {
        EpubError::Parse(msg) => {
            assert!(msg.contains("0 of 2"), "diagnostic missing counts: {msg}");
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn oversized_chapter_entry_rejected() {
    // A chapter that inflates past the per-entry cap must be refused instead
    // of slurped into memory (zip-bomb guard).
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata/>
  <manifest><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
    let mut big = b"<html><body><p>".to_vec();
    big.resize(17 * 1024 * 1024, b'a');
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
            ("OEBPS/ch1.xhtml", &big),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    let err = parse_epub_with_strategy(&buf, HeadingStrategy::Kobo)
        .expect_err("oversized chapter must be rejected");
    match err {
        EpubError::Parse(msg) => assert!(msg.contains("exceeds"), "got: {msg}"),
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn dot_segment_spine_hrefs_resolve() {
    // `./ch1.xhtml` hrefs must hit the same zip entry as `ch1.xhtml` — zip
    // lookups are byte-literal.
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata/>
  <manifest>
    <item id="nav" href="./nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1"  href="./ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
    let nav = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body>
  <nav epub:type="toc">
    <ol><li><a href="./ch1.xhtml">Dotted Chapter</a></li></ol>
  </nav>
</body></html>"#;
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body><p><span class="koboSpan">Dot-segment body.</span></p></body></html>"#;
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

    let chapters = parse_epub_with_strategy(&buf, HeadingStrategy::Kobo).expect("parse dotted");
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0].title, "Dotted Chapter");
    assert_eq!(chapters[0].body, "Dot-segment body.");
}
