import type { LibraryEntry } from "../../src/lib/ipc/bindings";

// A complete LibraryEntry so specs override only the field under test; the
// stub returns these verbatim from cmd_library_index. "done" (synced to
// LingQ) is the default because most specs exercise the course-link path,
// which needs a real lingq_collection_id to follow.
export function libraryEntry(
  contentHash: string,
  overrides: Partial<LibraryEntry> = {},
): LibraryEntry {
  return {
    id: {
      content_hash: contentHash,
      audible_asin: null,
      isbn13: null,
      calibre_uuid: null,
    },
    title: "Fixture Book",
    language: "en",
    completed_lesson_count: 0,
    receipt_count: 0,
    mtime: null,
    cover_path: null,
    authors: [],
    series: null,
    lingq_collection_id: null,
    last_activity_at: null,
    status: "done",
    failed_reason: null,
    ...overrides,
  };
}
