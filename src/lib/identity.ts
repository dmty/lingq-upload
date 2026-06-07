import type { ProjectId } from "$lib/ipc/bindings";

// Keep precedence aligned with `core/identity.rs::ProjectId::join_key`:
// asin > isbn13 > calibre_uuid > content_hash.
// `content_hash` arrives as a lowercase hex string from the backend
// (`#[serde(with = "hex_array_32")]`).
export function joinKey(id: ProjectId): string {
  if (id.audible_asin) return `asin:${id.audible_asin}`;
  if (id.isbn13) return `isbn13:${id.isbn13}`;
  if (id.calibre_uuid) return `uuid:${id.calibre_uuid}`;
  return `ch:${id.content_hash}`;
}
