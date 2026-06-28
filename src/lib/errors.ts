import type {
  AppError,
  AudioError,
  IngestError,
  LingqError,
  MappingError,
  SecretError,
  TextError,
} from "$lib/ipc/bindings";

export function secretMessage(e: SecretError): string {
  switch (e.kind) {
    case "LockedKeychain":
      return "Your OS keychain is locked.";
    case "UserDenied":
      return "Keychain access denied.";
    case "MissingEntry":
      return "No saved API key.";
    case "Backend":
      return `Keychain: ${e.message}`;
  }
}

export function lingqMessage(e: LingqError): string {
  switch (e.kind) {
    case "Unauthorized":
      return "LingQ rejected the API key.";
    case "NotFound":
      return "LingQ resource not found (check collection ID and language).";
    case "BadRequest":
      return `LingQ bad request: ${e.message}`;
    case "Server":
      return `LingQ server error: ${e.message}`;
    case "Schema":
      return `LingQ response schema: ${e.message}`;
    case "Transport":
      return `Network: ${e.message}`;
    case "Io":
      return `I/O: ${e.message}`;
  }
}

export function audioMessage(e: AudioError): string {
  switch (e.kind) {
    case "FfmpegNotFound":
      return `ffmpeg not found at ${e.message}`;
    case "FfmpegFailed":
      return `ffmpeg exited ${e.message.status}: ${e.message.stderr}`;
    case "Probe":
      return `ffprobe: ${e.message}`;
    case "DurationMismatch":
      return `Transcode duration mismatch (delta ${e.message.delta_sec}s)`;
    case "Io":
      return `I/O: ${e.message}`;
    case "Cancelled":
      return "Transcode cancelled";
  }
}

export function textErrorMessage(e: TextError): string {
  return `Text: ${e.message}`;
}

export function ingestMessage(e: IngestError): string {
  switch (e.kind) {
    case "NotSupported":
      return "This ingest source is not supported.";
    case "Io":
    case "Parse":
    case "Other":
      return `Ingest: ${e.message}`;
  }
}

export function mappingMessage(e: MappingError): string {
  switch (e.kind) {
    case "UnknownChapter":
      return `Unknown chapter: ${e.message}`;
    case "UnknownTrack":
      return `Unknown track: ${e.message}`;
    case "Invalid":
      return `Invalid mapping op: ${e.message}`;
  }
}

export function appErrorMessage(e: AppError): string {
  switch (e.kind) {
    case "Io":
      return `I/O error: ${e.message}`;
    case "Internal":
      return e.message;
    case "MissingApiKey":
      return "No LingQ API key configured. Open Settings to add one.";
    case "Unsupported":
      return e.message;
    case "Secrets":
      return secretMessage(e.message);
    case "Lingq":
      return lingqMessage(e.message);
    case "Audio":
      return audioMessage(e.message);
    case "Text":
      return textErrorMessage(e.message);
    case "Ingest":
      return ingestMessage(e.message);
    case "Mapping":
      return mappingMessage(e.message);
    case "MappingStaleOp":
      return `Mapping changed since last sync (server op ${e.message.server}, expected ${e.message.expected}). Reloading.`;
    case "Other":
      return e.message;
  }
}
