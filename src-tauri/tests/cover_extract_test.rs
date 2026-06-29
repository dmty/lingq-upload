use std::path::Path;
use lingq_upload_lib::core::epub::cover::{extract_to_dir, ExtractedCover};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/epub-covers")
        .join(name)
}

#[test]
fn extracts_via_epub3_properties() {
    let dir = tempfile::tempdir().unwrap();
    let ExtractedCover { path, mime, source_spine_href } =
        extract_to_dir(&fixture("epub3-properties.epub"), dir.path())
            .unwrap()
            .expect("cover present");
    assert!(path.starts_with(dir.path()));
    assert_eq!(path.file_name().unwrap().to_string_lossy(), "cover.jpg");
    assert_eq!(mime, "image/jpeg");
    assert!(std::fs::metadata(&path).unwrap().len() > 0);
    // EPUB3 fixture has no cover XHTML host page in spine — source href is None.
    assert!(source_spine_href.is_none());
}

#[test]
fn extracts_via_epub2_meta_cover() {
    let dir = tempfile::tempdir().unwrap();
    let cov = extract_to_dir(&fixture("epub2-meta-cover.epub"), dir.path())
        .unwrap()
        .expect("cover present");
    assert_eq!(cov.mime, "image/jpeg");
    // The fixture's spine contains a cover.xhtml that wraps the image.
    assert_eq!(cov.source_spine_href.as_deref(), Some("cover.xhtml"));
}

#[test]
fn extracts_via_guide_reference_with_inner_img() {
    let dir = tempfile::tempdir().unwrap();
    let cov = extract_to_dir(&fixture("guide-xhtml-img.epub"), dir.path())
        .unwrap()
        .expect("cover present");
    assert_eq!(cov.mime, "image/jpeg");
    assert_eq!(cov.source_spine_href.as_deref(), Some("cover.xhtml"));
}

#[test]
fn returns_none_when_no_cover() {
    // Build a minimal EPUB in memory with NO cover indicators at all.
    let dir = tempfile::tempdir().unwrap();
    let epub_path = dir.path().join("nocov.epub");
    let mut zw = zip::ZipWriter::new(std::fs::File::create(&epub_path).unwrap());
    use zip::write::SimpleFileOptions;
    zw.start_file("mimetype", SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
    use std::io::Write;
    zw.write_all(b"application/epub+zip").unwrap();
    zw.start_file("META-INF/container.xml", SimpleFileOptions::default()).unwrap();
    zw.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    zw.start_file("content.opf", SimpleFileOptions::default()).unwrap();
    zw.write_all(br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">x</dc:identifier><dc:title>t</dc:title><dc:language>en</dc:language></metadata><manifest><item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="ch1"/></spine></package>"#).unwrap();
    zw.start_file("ch1.xhtml", SimpleFileOptions::default()).unwrap();
    zw.write_all(b"<html><body><p>hi</p></body></html>").unwrap();
    zw.finish().unwrap();

    let out_dir = tempfile::tempdir().unwrap();
    let res = extract_to_dir(&epub_path, out_dir.path()).unwrap();
    assert!(res.is_none());
}
