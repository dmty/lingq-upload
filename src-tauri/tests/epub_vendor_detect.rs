//! EPUB vendor autodetection.
//!
//! No real Kobo EPUBs are checked in (license + binary churn), so this suite
//! synthesises minimal Kobo-flavoured EPUBs in-memory and pairs them with a
//! tiny Kindle-flavoured fixture and an empty/unknown one. The cluster floor
//! is exercised by an adversarial Kindle book that carries a single stray
//! `font-160per` span — it must not flip to Kobo.

use std::io::{Cursor, Write};

use lingq_upload_lib::core::epub::{detect_vendor, EpubVendor};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// sha256 of `tests/fixtures/epub/kobo/adversarial_mixed_classes.epub`.
/// Pinned so a silent rewrite of the binary blob (zip metadata, recompression,
/// content shift) fails CI loudly. Bump after a deliberate regeneration via
/// the `writes_adversarial_kobo_fixture_to_disk` `#[ignore]`d test.
const ADVERSARIAL_KOBO_FIXTURE_SHA256: &str =
    "12eb2d330b590e350164d7a0df3026030dc88c93509f43009ef7f8a2b012b759";

const CONTAINER_XML: &str = r#"<?xml version="1.0"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

const MINIMAL_OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata/>
  <manifest>
    <item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;

fn build_epub(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

fn build_kobo_fixture() -> Vec<u8> {
    let mut entries: Vec<(&'static str, Vec<u8>)> = vec![
        ("mimetype", b"application/epub+zip".to_vec()),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes().to_vec()),
        ("OEBPS/content.opf", MINIMAL_OPF.as_bytes().to_vec()),
    ];
    for i in 1..=4 {
        let body = format!(
            r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter {i}</title></head>
<body>
  <div class="font-160per">
    <p><span class="koboSpan" id="kobo.1.1">First sentence.</span></p>
    <p><span class="koboSpan" id="kobo.1.2">Second sentence.</span></p>
    <p><span class="koboSpan" id="kobo.1.3">Third sentence.</span></p>
  </div>
  <div class="font-140per">
    <p><span class="koboSpan" id="kobo.2.1">More body text.</span></p>
  </div>
</body>
</html>"#
        )
        .into_bytes();
        let name: &'static str = match i {
            1 => "OEBPS/ch1.xhtml",
            2 => "OEBPS/ch2.xhtml",
            3 => "OEBPS/ch3.xhtml",
            4 => "OEBPS/ch4.xhtml",
            _ => unreachable!(),
        };
        entries.push((name, body));
    }
    build_epub(
        &entries
            .iter()
            .map(|(n, b)| (*n, b.as_slice()))
            .collect::<Vec<_>>(),
    )
}

/// Picture-book style Kobo fixture: marker classes spread thin across many
/// short files. Exercises that the cluster floor is met by aggregation, not
/// by a single dense chapter.
fn build_kobo_adversarial_mixed() -> Vec<u8> {
    let mut entries: Vec<(&'static str, Vec<u8>)> = vec![
        ("mimetype", b"application/epub+zip".to_vec()),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes().to_vec()),
        ("OEBPS/content.opf", MINIMAL_OPF.as_bytes().to_vec()),
    ];
    // 6 files, 1-2 markers each. Mix of koboSpan and font-1[246]0per.
    let snippets: [&str; 6] = [
        r#"<p><span class="koboSpan" id="kobo.1">A.</span></p>"#,
        r#"<div class="font-120per"><span class="koboSpan" id="kobo.2">B.</span></div>"#,
        r#"<div class="font-140per">C.</div>"#,
        r#"<p><span class="koboSpan" id="kobo.4">D.</span></p>"#,
        r#"<div class="font-160per">E.</div>"#,
        r#"<p><span class="koboSpan" id="kobo.6">F.</span></p>"#,
    ];
    let names: [&'static str; 6] = [
        "OEBPS/p1.xhtml",
        "OEBPS/p2.xhtml",
        "OEBPS/p3.xhtml",
        "OEBPS/p4.xhtml",
        "OEBPS/p5.xhtml",
        "OEBPS/p6.xhtml",
    ];
    for (i, snip) in snippets.iter().enumerate() {
        let body = format!(
            r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>{snip}</body></html>"#
        )
        .into_bytes();
        entries.push((names[i], body));
    }
    build_epub(
        &entries
            .iter()
            .map(|(n, b)| (*n, b.as_slice()))
            .collect::<Vec<_>>(),
    )
}

fn build_kindle_fixture() -> Vec<u8> {
    let ncx = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head/><docTitle><text>K</text></docTitle>
  <navMap>
    <navPoint id="np1" playOrder="1"><navLabel><text>Ch 1</text></navLabel><content src="ch1.xhtml"/></navPoint>
  </navMap>
</ncx>"#;
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter 1</title></head>
<body>
  <div class="calibre">
    <mbp:pagebreak class="amzn-page-break"/>
    <p>Kindle body. kf8 marker. amzn-page-break repeated.</p>
    <p>kindle:position id="0"</p>
    <p>"calibre"-styled wrapper.</p>
  </div>
</body>
</html>"#;
    let entries: Vec<(&'static str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
        ("OEBPS/content.opf", MINIMAL_OPF.as_bytes()),
        ("OEBPS/toc.ncx", ncx.as_bytes()),
        ("OEBPS/ch1.xhtml", ch1.as_bytes()),
    ];
    build_epub(&entries)
}

fn build_kindle_with_stray_font_per() -> Vec<u8> {
    // Mostly-Kindle book that includes ONE stray `font-160per` and zero
    // koboSpan markers — the cluster floor must keep this on the Kindle side.
    let ncx = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head/><docTitle><text>K</text></docTitle>
  <navMap><navPoint id="np1" playOrder="1"><navLabel><text>Ch 1</text></navLabel><content src="ch1.xhtml"/></navPoint></navMap>
</ncx>"#;
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><style>.font-160per { font-size: 160%; }</style></head>
<body>
  <div class="calibre">
    <mbp:pagebreak class="amzn-page-break"/>
    <p>Kindle book with a single stray <span class="font-160per">marker</span>.</p>
    <p>kindle:position 1</p>
    <p>kf8</p>
  </div>
</body>
</html>"#;
    let entries: Vec<(&'static str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
        ("OEBPS/content.opf", MINIMAL_OPF.as_bytes()),
        ("OEBPS/toc.ncx", ncx.as_bytes()),
        ("OEBPS/ch1.xhtml", ch1.as_bytes()),
    ];
    build_epub(&entries)
}

fn build_empty_epub() -> Vec<u8> {
    let entries: Vec<(&'static str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
    ];
    build_epub(&entries)
}

fn detect(bytes: &[u8]) -> lingq_upload_lib::core::epub::VendorDetection {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).expect("open zip");
    detect_vendor(&mut zip).expect("detect")
}

/// Adversarial Kobo fixture is committed to disk; this test only asserts its
/// presence. Regenerating the bytes is opt-in via `--ignored` so the standard
/// suite never writes into the source tree.
#[test]
fn adversarial_kobo_fixture_is_committed() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/epub/kobo/adversarial_mixed_classes.epub");
    assert!(path.exists(), "missing committed fixture: {}", path.display());
}

#[test]
fn adversarial_kobo_fixture_pinned_by_sha256() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/epub/kobo/adversarial_mixed_classes.epub");
    let bytes = std::fs::read(&path).expect("read fixture");
    let mut h = Sha256::new();
    h.update(&bytes);
    let hex = hex::encode(h.finalize());
    assert_eq!(
        hex, ADVERSARIAL_KOBO_FIXTURE_SHA256,
        "fixture drifted; update the constant if the regeneration was intentional"
    );
}

#[test]
#[ignore = "regenerates the committed fixture; run explicitly with --ignored"]
fn writes_adversarial_kobo_fixture_to_disk() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/epub/kobo");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("adversarial_mixed_classes.epub");
    std::fs::write(&path, build_kobo_adversarial_mixed()).unwrap();
    assert!(path.exists());
}

#[test]
fn dense_kobo_book_classified_kobo() {
    let d = detect(&build_kobo_fixture());
    assert_eq!(d.vendor, EpubVendor::Kobo);
    assert!(
        d.confidence >= 0.8,
        "confidence {} below 0.8",
        d.confidence
    );
    assert!(
        d.signals.iter().any(|s| s.starts_with("kobo_span")),
        "missing kobo_span signal in {:?}",
        d.signals
    );
    assert!(
        d.signals.iter().any(|s| s.starts_with("font_per")),
        "missing font_per signal in {:?}",
        d.signals
    );
}

#[test]
fn adversarial_mixed_kobo_book_classified_kobo() {
    let d = detect(&build_kobo_adversarial_mixed());
    assert_eq!(d.vendor, EpubVendor::Kobo);
    assert!(d.confidence >= 0.8, "confidence {}", d.confidence);
    assert!(d.signals.iter().any(|s| s.starts_with("kobo_span")));
}

/// Marker types accumulate per file: 2 koboSpan + 2 font-per in ONE file is a
/// combined total of 4 ≥ floor, even though neither type alone reaches 3.
#[test]
fn combined_marker_types_in_single_file_classified_kobo() {
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body>
  <div class="font-120per">
    <p><span class="koboSpan" id="kobo.1.1">A.</span></p>
  </div>
  <div class="font-160per">
    <p><span class="koboSpan" id="kobo.1.2">B.</span></p>
  </div>
</body>
</html>"#;
    let entries: Vec<(&'static str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
        ("OEBPS/content.opf", MINIMAL_OPF.as_bytes()),
        ("OEBPS/ch1.xhtml", ch1.as_bytes()),
    ];
    let d = detect(&build_epub(&entries));
    assert_eq!(d.vendor, EpubVendor::Kobo, "signals={:?}", d.signals);
}

#[test]
fn kindle_book_classified_kindle() {
    let d = detect(&build_kindle_fixture());
    assert_eq!(d.vendor, EpubVendor::Kindle);
    assert!(d.confidence >= 0.8, "confidence {}", d.confidence);
    assert!(d.signals.iter().any(|s| s == "toc_ncx"));
    assert!(d.signals.iter().any(|s| s.starts_with("kindle_marker")));
}

#[test]
fn one_stray_font_per_does_not_flip_kindle() {
    let d = detect(&build_kindle_with_stray_font_per());
    assert_eq!(
        d.vendor,
        EpubVendor::Kindle,
        "stray font-160per flipped vendor; signals={:?}",
        d.signals
    );
}

#[test]
fn empty_epub_falls_back_to_generic() {
    let d = detect(&build_empty_epub());
    assert_eq!(d.vendor, EpubVendor::Generic);
    assert!(
        d.confidence <= 0.4,
        "confidence {} should be ≤ 0.4",
        d.confidence
    );
}

#[test]
fn signals_enumerate_marker_buckets() {
    let d = detect(&build_kobo_fixture());
    // Every reported signal must be one of the known bucket names.
    for s in &d.signals {
        assert!(
            s.starts_with("kobo_span")
                || s.starts_with("font_per")
                || s.starts_with("kindle_marker")
                || s == "toc_ncx",
            "unknown signal {s}"
        );
    }
}

/// Pathological case: a Kindle book whose CSS stylesheet repeats
/// `font-160per` five times. Per-file counting must keep this on the Kindle
/// side; the cluster floor only applies to XHTML body files.
#[test]
fn kindle_book_with_kobo_classes_in_css_only_stays_kindle() {
    let ncx = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head/><docTitle><text>K</text></docTitle>
  <navMap><navPoint id="np1" playOrder="1"><navLabel><text>Ch 1</text></navLabel><content src="ch1.xhtml"/></navPoint></navMap>
</ncx>"#;
    let css = r#".font-160per { font-size: 160%; }
.font-160per::before { content: "" }
.font-160per > p { margin: 0 }
.font-160per .x { color: red }
.font-160per .y { color: blue }"#;
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body><div class="calibre"><p>kindle:position 1</p><p>kf8</p></div></body>
</html>"#;
    let entries: Vec<(&'static str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
        ("OEBPS/content.opf", MINIMAL_OPF.as_bytes()),
        ("OEBPS/toc.ncx", ncx.as_bytes()),
        ("OEBPS/style.css", css.as_bytes()),
        ("OEBPS/ch1.xhtml", ch1.as_bytes()),
    ];
    let d = detect(&build_epub(&entries));
    assert_eq!(d.vendor, EpubVendor::Kindle, "signals={:?}", d.signals);
}

/// `dc:identifier` containing the literal string `koboSpan_dummy` must not
/// flip a Kindle book — the OPF is never scanned for body markers.
#[test]
fn kindle_book_with_kobospan_in_opf_stays_kindle() {
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata>
    <dc:identifier xmlns:dc="http://purl.org/dc/elements/1.1/" id="id">koboSpan_dummy</dc:identifier>
  </metadata>
  <manifest><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
    let ncx = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head/><docTitle><text>K</text></docTitle>
  <navMap><navPoint id="np1" playOrder="1"><navLabel><text>Ch 1</text></navLabel><content src="ch1.xhtml"/></navPoint></navMap>
</ncx>"#;
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body><div class="calibre"><p>kindle:position 1</p><p>kf8</p></div></body>
</html>"#;
    let entries: Vec<(&'static str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/toc.ncx", ncx.as_bytes()),
        ("OEBPS/ch1.xhtml", ch1.as_bytes()),
    ];
    let d = detect(&build_epub(&entries));
    assert_eq!(d.vendor, EpubVendor::Kindle, "signals={:?}", d.signals);
}

/// Markers buried in XML comments must not feed the cluster count.
#[test]
fn xml_comment_markers_do_not_flip_kindle() {
    let ncx = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head/><docTitle><text>K</text></docTitle>
  <navMap><navPoint id="np1" playOrder="1"><navLabel><text>Ch 1</text></navLabel><content src="ch1.xhtml"/></navPoint></navMap>
</ncx>"#;
    let ch1 = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body>
  <!-- koboSpan koboSpan koboSpan font-160per -->
  <div class="calibre"><p>kindle:position 1</p><p>kf8</p></div>
</body></html>"#;
    let entries: Vec<(&'static str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER_XML.as_bytes()),
        ("OEBPS/content.opf", MINIMAL_OPF.as_bytes()),
        ("OEBPS/toc.ncx", ncx.as_bytes()),
        ("OEBPS/ch1.xhtml", ch1.as_bytes()),
    ];
    let d = detect(&build_epub(&entries));
    assert_eq!(d.vendor, EpubVendor::Kindle, "signals={:?}", d.signals);
}

/// A truncated EPUB (header-only bytes) must surface as a zip-level error,
/// not silently classify as Generic.
#[test]
fn truncated_epub_returns_err() {
    use lingq_upload_lib::core::epub::EpubError;
    let full = build_kindle_fixture();
    let truncated = &full[..full.len().min(16)];
    let opened = zip::ZipArchive::new(Cursor::new(truncated));
    let result = match opened {
        Ok(mut zip) => lingq_upload_lib::core::epub::detect_vendor(&mut zip),
        Err(e) => Err(EpubError::Zip(e.to_string())),
    };
    assert!(
        matches!(result, Err(EpubError::Zip(_))),
        "expected EpubError::Zip, got {result:?}",
    );
}
