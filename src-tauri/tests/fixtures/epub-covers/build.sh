#!/usr/bin/env bash
# Regenerates the three EPUB cover fixtures from inline templates.
# Run from repo root: bash src-tauri/tests/fixtures/epub-covers/build.sh
set -euo pipefail
cd "$(dirname "$0")"

# 1x1 px JPEG — base64 of a minimal valid JPEG (~125 bytes decoded).
JPEG_B64="/9j/4AAQSkZJRgABAQEASABIAAD/2wBDAAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQH/2wBDAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQH/wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAr/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8QAFQEBAQAAAAAAAAAAAAAAAAAAAAX/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIRAxEAPwA/wD/2Q=="

mkdir -p _build
cd _build

# Shared files.
echo -n "application/epub+zip" > mimetype
mkdir -p META-INF
cat > META-INF/container.xml <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
EOF
echo "$JPEG_B64" | base64 -d > cover.jpg
cat > chapter1.xhtml <<EOF
<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Ch1</title></head>
<body><h1>Chapter 1</h1><p>Body text for chapter one.</p></body></html>
EOF

# SIMPLIFY: Factor repeated zip sequence into helper function
build_epub() {
  local name=$1
  rm -f "../$name"
  zip -X0 "../$name" mimetype >/dev/null
  zip -Xr9 "../$name" META-INF content.opf cover.jpg "$@" >/dev/null
}

# Fixture A: EPUB3 properties=cover-image
cat > content.opf <<EOF
<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:test-a</dc:identifier>
    <dc:title>Fixture A</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="cov" href="cover.jpg" media-type="image/jpeg" properties="cover-image"/>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>
EOF
build_epub "epub3-properties.epub" chapter1.xhtml

# Fixtures B & C: EPUB2 variants with cover.xhtml. Body carries a real
# paragraph so the cover chapter is not empty-dropped by the parser — the
# cover-filter behaviour we test only matters when cover.xhtml survives parse.
cat > cover.xhtml <<EOF
<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Cover</title></head>
<body><p>Cover image follows.</p><img src="cover.jpg" alt="cover"/></body></html>
EOF

# Fixture B: meta name=cover
cat > content.opf <<EOF
<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:test-b</dc:identifier>
    <dc:title>Fixture B</dc:title>
    <dc:language>en</dc:language>
    <meta name="cover" content="cov"/>
  </metadata>
  <manifest>
    <item id="cov" href="cover.jpg" media-type="image/jpeg"/>
    <item id="covpg" href="cover.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="covpg"/><itemref idref="ch1"/></spine>
</package>
EOF
build_epub "epub2-meta-cover.epub" cover.xhtml chapter1.xhtml

# Fixture C: guide reference points to cover.xhtml
cat > content.opf <<EOF
<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:test-c</dc:identifier>
    <dc:title>Fixture C</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="cov" href="cover.jpg" media-type="image/jpeg"/>
    <item id="covpg" href="cover.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="covpg"/><itemref idref="ch1"/></spine>
  <guide><reference href="cover.xhtml" type="cover" title="Cover"/></guide>
</package>
EOF
build_epub "guide-xhtml-img.epub" cover.xhtml chapter1.xhtml

cd ..
rm -rf _build
echo "Built: $(ls *.epub)"
