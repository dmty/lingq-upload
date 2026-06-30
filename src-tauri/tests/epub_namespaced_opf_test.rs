//! Regression: Sigil/Calibre EPUBs declare `xmlns:opf` on `<manifest>` and
//! use `<opf:item .../>` for every manifest entry. The OPF reader previously
//! compared element names against raw byte slices (`name.as_ref() == b"item"`)
//! and missed the namespaced form, producing empty manifests and 0 chapters.
//! See Roadside Picnic (Sigil 0.4.0 + Calibre 5.17.0).

use std::io::Write;

use lingq_upload_lib::core::epub::parse_epub;
use zip::write::SimpleFileOptions;

fn build_sigil_style_epub(path: &std::path::Path) {
    let mut zw = zip::ZipWriter::new(std::fs::File::create(path).unwrap());

    zw.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zw.write_all(b"application/epub+zip").unwrap();

    zw.start_file("META-INF/container.xml", SimpleFileOptions::default())
        .unwrap();
    zw.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
    )
    .unwrap();

    // Mimics Sigil/Calibre output: <manifest xmlns:opf="..."> with <opf:item> children.
    zw.start_file("content.opf", SimpleFileOptions::default())
        .unwrap();
    zw.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="id" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:sigil-style</dc:identifier>
    <dc:title>Sigil Sample</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest xmlns:opf="http://www.idpf.org/2007/opf">
    <opf:item href="Text/ch1.xhtml" id="ch1" media-type="application/xhtml+xml"/>
    <opf:item href="Text/ch2.xhtml" id="ch2" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
    <itemref idref="ch2"/>
  </spine>
</package>"#,
    )
    .unwrap();

    for (name, body) in [
        ("Text/ch1.xhtml", "<h1>Chapter One</h1><p>Body of chapter one.</p>"),
        ("Text/ch2.xhtml", "<h1>Chapter Two</h1><p>Body of chapter two.</p>"),
    ] {
        zw.start_file(name, SimpleFileOptions::default()).unwrap();
        let xhtml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>x</title></head><body>{body}</body></html>"#
        );
        zw.write_all(xhtml.as_bytes()).unwrap();
    }

    zw.finish().unwrap();
}

#[test]
fn namespaced_opf_item_manifest_parses_chapters() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sigil.epub");
    build_sigil_style_epub(&path);

    let chapters = parse_epub(&path).expect("parse_epub ok");
    assert_eq!(
        chapters.len(),
        2,
        "expected 2 chapters from spine, got {}",
        chapters.len()
    );
    assert_eq!(chapters[0].spine_href, "Text/ch1.xhtml");
    assert_eq!(chapters[1].spine_href, "Text/ch2.xhtml");
}
