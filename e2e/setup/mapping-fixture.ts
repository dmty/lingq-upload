import type { Page } from "@playwright/test";

import type {
  Chapter,
  MappingPair,
  MappingState,
  MismatchInspection,
} from "../../src/lib/ipc/bindings";
import { seed } from "./test";

export const chapters = (count: number, bodyChars = 100): Chapter[] =>
  Array.from({ length: count }, (_, i) => ({
    id: `idx:${i}`,
    order: i,
    title: `Chapter ${i + 1}`,
    body: "x".repeat(bodyChars),
    kind: "body",
  }));

export const pair = (
  index: number,
  trackId: string,
  confidence: number,
  originalConfidence = confidence,
): MappingPair => ({
  chapter_id: `idx:${index}`,
  track_id: trackId,
  confidence,
  original_confidence: originalConfidence,
  touched: false,
});

// Both stores are installed by the stub — __pickerState__ with a
// sessionStorage-backed skippedByProject getter, __mappingState__ with a
// deliberately non-destructive seed() — so a spec seeds *through* them
// instead of replacing either wholesale. Overwriting either would delete the
// rehydration behaviour a mid-test page.reload exercises.
export const seedChapters = (page: Page, key: string, list: Chapter[]) =>
  page.addInitScript(
    (c) => {
      window.__pickerState__.chaptersByProject[c.key] = c.list;
    },
    { key, list },
  );

export const seedMapping = (page: Page, key: string, mapping: MappingState) =>
  page.addInitScript((m) => window.__mappingState__.seed(m.key, m.mapping), {
    key,
    mapping,
  });

// One call per spec: the two stateful seams plus the plain-data globals.
export async function installMapping(
  page: Page,
  opts: {
    key: string;
    chapters: Chapter[];
    mapping: MappingState;
    inspection?: MismatchInspection | null;
  },
): Promise<void> {
  // A spec that passes `inspection: null` explicitly is asserting on the
  // empty inspection itself; one that omits it just doesn't care and rides
  // this default. Keep that distinction at call sites — don't "clean up" an
  // explicit null into an omitted field.
  await seed(page, { __matcherInspection__: opts.inspection ?? null });
  await seedChapters(page, opts.key, opts.chapters);
  await seedMapping(page, opts.key, opts.mapping);
}
