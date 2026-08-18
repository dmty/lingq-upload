// The stub↔spec contract. Every fixture a spec seeds and every seam it reads
// back is declared here, so a fixture that drifts from the Rust-generated
// bindings fails `bun run check:e2e` instead of a screen assertion.
import type {
  AppError,
  AppTranscriptionPreferences,
  Chapter,
  ChapterReceipt,
  Collection,
  CourseView,
  DetectStartResult,
  DetectedRange,
  DetectionAvailability,
  DetectionPreview,
  LibraryEntry,
  Language,
  MappingState,
  MatcherDecision,
  MismatchInspection,
  PlanStep,
  Project,
  ProjectId,
  TranscribeProviderId,
} from "../../src/lib/ipc/bindings";

type Fixture = Record<string, unknown>;
// Shapes that recur across the fixtures below, named once so each
// declaration reads as what it is rather than its structure.
type Gate = Promise<void>;
type Release = () => void;

declare global {
  interface Window {
    // Tauri runtime the stub replaces.
    __TAURI_INTERNALS__: {
      metadata: {
        currentWindow: { label: string };
        currentWebview: { label: string };
      };
      invoke: (cmd: string, args?: unknown) => Promise<unknown>;
      transformCallback: (cb: (payload: unknown) => void) => number;
      convertFileSrc: (path: string) => string;
    };
    __TAURI_EVENT_PLUGIN_INTERNALS__: {
      unregisterListener: (event: string, id: number) => void;
    };

    // Event seam: the stub records listeners, specs fire into them.
    __eventHandlers__: Record<string, number[]>;
    __emitEvent__: (event: string, payload: unknown) => void;
    __invokeLog__: string[];

    // State objects the stub installs; specs seed through them.
    __pickerState__: {
      readonly skippedByProject: Record<string, string[]>;
      chaptersByProject: Record<string, Chapter[]>;
      _writeSkipped: (m: Record<string, string[]>) => void;
    };
    __mappingState__: {
      readonly byProject: Record<string, MappingState>;
      seed: (key: string, state: MappingState) => void;
    };

    // Library / course fixtures.
    __libraryEntries__?: LibraryEntry[];
    __libraryError__?: AppError | Error;
    __libraryGate__?: Gate;
    __releaseLibrary__?: Release;
    __courseView__?: CourseView;
    __courseError__?: AppError | Error;
    __courseGate__?: Gate;
    __releaseCourse__?: Release;
    __collections__?: Collection[];
    __languages__?: Language[];
    __openedUrl__?: string;
    __lingqKey__?: string | null;

    // Project fixtures.
    __projectByKey__?: Record<string, Project>;
    __planByKey__?: Record<string, PlanStep[]>;
    __projectMeta__?: Record<
      string,
      { title?: string; cover_path?: string | null; authors?: string[] }
    >;
    __receiptsByProject__?: Record<string, ChapterReceipt[]>;
    __matcherDecisionByProject__?: Record<string, MatcherDecision | null>;
    __matcherInspection__?: MismatchInspection | null;
    __matcherInspectionByProject__?: Record<string, MismatchInspection | null>;
    __matcherSeedByProject__?: Record<string, Fixture>;
    __failNextMappingOp__?: boolean;
    __confirmMappingCalls__?: { projectId: ProjectId }[];

    // Upload fixtures.
    __uploadOneShotResult__?: { lesson_id: number; collection_id: number };
    __uploadOneShotError__?: AppError | Error;
    __uploadOneShotGate__?: Gate;
    __releaseUpload__?: Release;
    __cancelJobCalls__?: { jobId: string }[];

    // Transcription / detection fixtures.
    __transcriptionPreferences__?: AppTranscriptionPreferences;
    __transcriptionKeys__?: Record<string, boolean>;
    __transcriptionConsents__?: Record<string, boolean>;
    __failNextTranscriptionPreferences__?: boolean;
    __failNextTranscribeConsent__?: boolean;
    __transcribeConsentCalls__?: {
      projectId: ProjectId;
      providerId: TranscribeProviderId;
    }[];
    __transcribeConsentGate__?: Gate;
    __releaseTranscribeConsent__?: Release;
    __detectionAvailability__?: DetectionAvailability;
    __detectionAvailabilityByProject__?: Record<string, DetectionAvailability>;
    __detectionResult__?: DetectStartResult;
    __detectionCommandError__?: AppError | Error;
    __detectionGate__?: Gate;
    __releaseDetection__?: Release;
    __detectionStartCalls__?: number;
    __detectionStartArgs__?: { projectId: ProjectId; jobId: string }[];
    __detectionListenerCountAtStart__?: number;
    __confirmDetectedRangeCalls__?: {
      projectId: ProjectId;
      selectedRange: DetectedRange;
      evidence: DetectionPreview;
    }[];
    __confirmDetectedRangeError__?: AppError | Error;
    __confirmDetectedRangeGate__?: Gate;
    __resetDetectionCalls__?: { projectId: ProjectId }[];
    __resetDetectionError__?: AppError | Error;
    __resetDetectionGate__?: Gate;
    __dialogPickPath__?: string | null;
  }
}
