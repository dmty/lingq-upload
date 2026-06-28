import { convertFileSrc } from "@tauri-apps/api/core";

export function assetUrl(path: string): string {
  return convertFileSrc(path);
}

// Custom `audio://` URI scheme registered in `src-tauri/src/audio_scheme.rs`.
// Required because WebKit on macOS refuses `<audio>` playback over Tauri's
// built-in asset:// protocol for AAC/m4b streams.
export function audioUrl(path: string): string {
  return "audio://localhost/" + encodeURIComponent(path);
}
