import { convertFileSrc } from "@tauri-apps/api/core";

export function assetUrl(path: string): string {
  return convertFileSrc(path);
}

// Map a file path's extension to a MIME hint for `<source type=...>`.
// Tauri's asset protocol falls back to `application/octet-stream` for
// extensions outside `mime_guess`'s table — notably `.m4b` — and WebKit
// then refuses to decode the stream. A static type hint sidesteps that.
export function audioMime(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "m4a":
    case "m4b":
    case "mp4":
    case "aac":
      return "audio/mp4";
    case "mp3":
      return "audio/mpeg";
    case "ogg":
    case "opus":
      return "audio/ogg";
    case "flac":
      return "audio/flac";
    case "wav":
      return "audio/wav";
    default:
      return "";
  }
}
