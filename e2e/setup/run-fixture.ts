import type { PlanStep, Project } from "../../src/lib/ipc/bindings";

// Shared fixture builder for the run screen's specs.
//
// A run-screen spec needs two stub hooks in agreement: `__projectByKey__`
// (what `cmd_project_load` returns) and `__planByKey__` (what
// `cmd_project_plan_preview` returns). Keeping them in one place stops the
// two drifting apart as the Project shape grows a field.

export type PlanStepFixture = {
  chapter_index: number;
  title: string;
  degraded?: boolean;
};

export type ReceiptFixture = {
  chapter_index: number;
  lesson_id?: number | null;
  uploaded_at?: string | null;
  degraded?: boolean;
};

export function runFixture(opts: {
  key: string;
  title: string;
  plan?: PlanStepFixture[];
  receipts?: ReceiptFixture[];
  lingqCollectionId?: number | null;
}): Partial<Window> {
  const { key, title, plan = [], receipts = [], lingqCollectionId = 42 } = opts;

  const project: Project = {
    schema_version: 1,
    id: {
      content_hash: key,
      audible_asin: null,
      isbn13: null,
      calibre_uuid: null,
    },
    sources: { text: { kind: "missing" }, audio: null },
    settings: { language: "en", collection_title: title, level: 1, tags: [] },
    receipts: receipts.map((r) => ({
      chapter_index: r.chapter_index,
      lesson_id: r.lesson_id ?? null,
      uploaded_at: r.uploaded_at ?? null,
      degraded: r.degraded ?? false,
    })),
    queue_cursor: 0,
    completed_lesson_ids: [],
    matcher_decision: null,
    cover_path: null,
    authors: [],
    series: null,
    lingq_collection_id: lingqCollectionId,
    last_activity_at: null,
    stage: "mapped",
    last_transition_at: null,
    skipped_chapters: [],
    mapping: null,
    confirmed_at: "2026-01-01T00:00:00Z",
  };

  const steps: PlanStep[] = plan.map((s) => ({
    chapter_index: s.chapter_index,
    title: s.title,
    degraded: s.degraded ?? false,
  }));

  return {
    __projectByKey__: { [key]: project },
    __planByKey__: { [key]: steps },
  };
}
