import type {
  AppError,
  AudioError,
  DetectedRangeError,
  IngestError,
  LingqError,
  MappingError,
  SecretError,
  TextError,
  TranscribeError,
  TranscribeErrorKind,
} from "$lib/ipc/bindings";

const GENERIC_MESSAGE = "Something went wrong. Try again.";

// Rejections cross the IPC boundary untyped — a bad argument or an unknown
// command arrives as a plain string, not an AppError.
function isAppError(value: unknown): value is AppError {
  return typeof value === "object" && value !== null && "kind" in value;
}

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

export function transcribeMessage(e: TranscribeError): string {
  switch (e.kind) {
    case "api_key":
      return "No transcription API key configured. Open Settings to add one.";
    case "unauthorized":
      return "The transcription provider rejected the API key. Edit it in Settings.";
    case "rate_limit":
      return "The transcription provider rate limit was reached. Try again later or switch providers.";
    case "timeout":
      return "Transcription timed out. Try again.";
    case "network":
      return "Couldn't reach the transcription provider. Check your internet connection and try again.";
    case "provider_failed":
      return "The transcription provider failed. Try again or switch providers.";
    case "audio":
      return "Couldn't prepare this audio for transcription. Re-add it or check the file.";
    default:
      return "Transcription failed. Try again or switch providers.";
  }
}

export function detectedRangeMessage(e: DetectedRangeError): string {
  switch (e.kind) {
    case "MissingBoundary":
    case "DuplicateBoundary":
      return e.message;
    case "EndBeforeStart":
      return "The detected end chapter comes before the start chapter.";
    case "EmptyRange":
      return "The detected chapter range is empty.";
    default:
      return "The detected chapter range is not usable. Re-run detection or choose a manual response.";
  }
}

export function appErrorMessage(e: unknown): string {
  if (!isAppError(e)) {
    const text = typeof e === "string" ? e.trim() : "";
    return text || GENERIC_MESSAGE;
  }
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
    case "DetectedRange":
      switch (e.message.kind) {
        case "MissingBoundary":
        case "DuplicateBoundary":
          return e.message.message;
        case "EndBeforeStart":
          return "The detected end chapter comes before the start chapter.";
        case "EmptyRange":
          return "The detected chapter range is empty.";
      }
    case "Transcribe":
      return transcribeMessage(e.message);
    case "Other":
      return e.message;
  }
}

/** Recovery affordances an operational failure justifies offering. */
export type RecoveryAction =
  | "settings"
  | "switch_provider"
  | "retry"
  | "manual";

export function transcribeActions(kind: TranscribeErrorKind): RecoveryAction[] {
  switch (kind) {
    case "api_key":
    case "unauthorized":
      return ["settings"];
    case "rate_limit":
    case "provider_failed":
      return ["switch_provider", "retry"];
    case "timeout":
    case "network":
      return ["retry"];
    case "audio":
      return ["manual"];
  }
}

export function appErrorActions(e: AppError): RecoveryAction[] {
  if (e.kind === "Transcribe") return transcribeActions(e.message.kind);
  if (e.kind === "Audio") return ["manual"];
  return ["retry"];
}

export function isMissingApiKey(e: AppError): boolean {
  return e.kind === "MissingApiKey";
}

export function isLingqNotFound(e: AppError): boolean {
  return e.kind === "Lingq" && e.message.kind === "NotFound";
}
