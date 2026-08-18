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
): MappingPair => ({
  chapter_id: `idx:${index}`,
  track_id: trackId,
  confidence,
  original_confidence: confidence,
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

export function mappingFixture(opts: {
  key: string;
  chapters?: Chapter[];
  mapping: MappingState;
  inspection?: MismatchInspection | null;
}) {
  return {
    chapters: opts.chapters ?? chapters(5),
    mapping: opts.mapping,
    globals: {
      __matcherInspection__: opts.inspection ?? null,
    } satisfies Partial<Window>,
  };
}

// One call per spec: the two stateful seams plus the plain-data globals.
export async function installMapping(
  page: Page,
  opts: Parameters<typeof mappingFixture>[0],
): Promise<void> {
  const fixture = mappingFixture(opts);
  await seed(page, fixture.globals);
  await seedChapters(page, opts.key, fixture.chapters);
  await seedMapping(page, opts.key, fixture.mapping);
}
