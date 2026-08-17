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
  DetectionAvailability,
  LibraryEntry,
  Language,
  MappingState,
  MatcherDecision,
  MismatchInspection,
  PlanStep,
  Project,
} from "../../src/lib/ipc/bindings";

type Fixture = Record<string, unknown>;

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
    __libraryGate__?: Promise<void>;
    __releaseLibrary__?: () => void;
    __courseView__?: CourseView;
    __courseError__?: AppError | Error;
    __courseGate__?: Promise<void>;
    __releaseCourse__?: () => void;
    __collections__?: Collection[];
    __languages__?: Language[];
    __openedUrl__?: string;
    __lingqKey__?: string | null;

    // Project fixtures.
    __projectByKey__?: Record<string, Project>;
    __planByKey__?: Record<string, PlanStep[]>;
    __projectMeta__?: Record<
      string,
      { cover_path?: string | null; authors?: string[] }
    >;
    __receiptsByProject__?: Record<string, ChapterReceipt[]>;
    __matcherDecisionByProject__?: Record<string, MatcherDecision | null>;
    __matcherInspection__?: MismatchInspection | null;
    __matcherInspectionByProject__?: Record<string, MismatchInspection | null>;
    __matcherSeedByProject__?: Record<string, Fixture>;
    __failNextMappingOp__?: boolean;
    __confirmMappingCalls__?: Fixture[];

    // Upload fixtures.
    __uploadOneShotResult__?: { lesson_id: number; collection_id: number };
    __uploadOneShotError__?: AppError | Error;
    __uploadOneShotGate__?: Promise<void>;
    __releaseUpload__?: () => void;
    __cancelJobCalls__?: { jobId?: string }[];

    // Transcription / detection fixtures.
    __transcriptionPreferences__?: AppTranscriptionPreferences;
    __transcriptionKeys__?: Record<string, boolean>;
    __transcriptionConsents__?: Record<string, boolean>;
    __failNextTranscriptionPreferences__?: boolean;
    __failNextTranscribeConsent__?: boolean;
    __transcribeConsentCalls__?: Fixture[];
    __transcribeConsentGate__?: Promise<void>;
    __releaseTranscribeConsent__?: () => void;
    __detectionAvailability__?: DetectionAvailability;
    __detectionAvailabilityByProject__?: Record<string, DetectionAvailability>;
    __detectionResult__?: DetectStartResult;
    __detectionCommandError__?: AppError | Error;
    __detectionGate__?: Promise<void>;
    __releaseDetection__?: () => void;
    __detectionStartCalls__?: number;
    __detectionStartArgs__?: { jobId?: string }[];
    __detectionListenerCountAtStart__?: number;
    __confirmDetectedRangeCalls__?: Fixture[];
    __confirmDetectedRangeError__?: AppError | Error;
    __confirmDetectedRangeGate__?: Promise<void>;
    __resetDetectionCalls__?: Fixture[];
    __resetDetectionError__?: AppError | Error;
    __resetDetectionGate__?: Promise<void>;
    __dialogPickPath__?: string | null;
  }
}
