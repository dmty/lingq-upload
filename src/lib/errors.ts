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
      return `LingQ rejected the request (${e.message}).`;
    case "Server":
      return "LingQ had a server problem. Try again in a minute.";
    case "Schema":
      return "LingQ sent an unexpected response. Try again; if it keeps happening, update the app.";
    case "Transport":
      return "Couldn't reach LingQ. Check your internet connection.";
    case "Io":
      return `File error: ${e.message}`;
  }
}

export function audioMessage(e: AudioError): string {
  switch (e.kind) {
    case "Decode":
      return `Couldn't read this audio file — it may be corrupt or DRM-protected (${e.message}).`;
    case "Encode":
      return `Couldn't convert the audio (${e.message}).`;
    case "Probe":
      return `Couldn't inspect the audio file (${e.message}).`;
    case "DurationMismatch":
      return `The converted audio came out ${Math.round(e.message.delta_sec)}s off the original. Try re-adding the file.`;
    case "Io":
      return `File error: ${e.message}`;
    case "Cancelled":
      return "Upload cancelled.";
  }
}

export function textErrorMessage(e: TextError): string {
  return `Couldn't process the book text (${e.message}).`;
}

export function ingestMessage(e: IngestError): string {
  switch (e.kind) {
    case "NotSupported":
      return "This source type isn't supported.";
    case "Io":
    case "Parse":
    case "Other":
      return `Couldn't read the book (${e.message}).`;
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
      return "This mapping was changed elsewhere. Reloading the latest version.";
    case "Other":
      return e.message;
  }
}

export function isMissingApiKey(e: AppError): boolean {
  return e.kind === "MissingApiKey";
}

export function isLingqNotFound(e: AppError): boolean {
  return e.kind === "Lingq" && e.message.kind === "NotFound";
}
