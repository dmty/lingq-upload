# EPUB fixtures

Raw EPUB files in this directory are **personal Kindle decrypts** and are not
shipped in the repository. They are gitignored. Tests that depend on them
skip cleanly when the files are absent so CI stays green.

## What lives here

- `kafka_shimo.epub` — 下巻 of 海辺のカフカ. Snapshot fixture for
  `parse_epub(HeadingStrategy::Kindle)`. Produced by
  `scripts/fixtures/extract_shimo.sh` from the combined 上下合本 EPUB.
- `dialects/` — additional Kindle / Kobo / generic EPUBs that exercise
  alternative heading strategies. Populated by `scripts/fixtures/seed-epub.sh`.
- `dialects/expected.json` — committed snapshot of the per-fixture parse
  expectations. Empty until seeded.

## Seeding locally

```sh
./scripts/fixtures/seed-epub.sh        # symlinks files from ~/Downloads
./scripts/fixtures/extract_shimo.sh    # produces kafka_shimo.epub
cargo test --manifest-path src-tauri/Cargo.toml --test kafka_snapshot_test
```

## What gets committed

Only derived JSON snapshots — never the EPUB source. Snapshots live alongside
their tests under `src-tauri/tests/snapshots/`.
