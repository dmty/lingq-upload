import type { ProjectId } from "$lib/ipc/bindings";

// Keep precedence aligned with `core/identity.rs::ProjectId::join_key`:
// asin > isbn13 > calibre_uuid > content_hash.
export function joinKey(id: ProjectId): string {
  if (id.audible_asin) return `asin:${id.audible_asin}`;
  if (id.isbn13) return `isbn13:${id.isbn13}`;
  if (id.calibre_uuid) return `uuid:${id.calibre_uuid}`;
  return `ch:${hexEncode(id.content_hash)}`;
}

function hexEncode(bytes: ReadonlyArray<number>): string {
  let out = "";
  for (const b of bytes) {
    out += (b & 0xff).toString(16).padStart(2, "0");
  }
  return out;
}
