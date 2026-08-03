import { untrack } from "svelte";
import {
  commands,
  type AppError,
  type CourseView,
} from "$lib/ipc/bindings";

export const STALE_AFTER_MS = 15 * 60 * 1000;

export type CourseEntry = {
  view: CourseView | null;
  fetchedAt: number | null;
  revalidating: boolean;
  error: AppError | null;
};

const entries = $state<Record<string, CourseEntry>>({});

function keyOf(lang: string, collectionId: number): string {
  return `${lang}:${collectionId}`;
}

function blank(): CourseEntry {
  return { view: null, fetchedAt: null, revalidating: false, error: null };
}

export const course = {
  entry(lang: string, collectionId: number): CourseEntry {
    return entries[keyOf(lang, collectionId)] ?? blank();
  },

  async ensure(lang: string, collectionId: number, force = false) {
    const key = keyOf(lang, collectionId);
    // Read outside the reactive graph: this runs from inside a caller's
    // `$effect`, and a tracked read here — followed by the write below —
    // would make every settle re-trigger that effect, which re-invokes
    // `ensure`, forever. `ensure` is an imperative action, not a derivation.
    const current = untrack(() => entries[key]);
    if (current?.revalidating) return;
    entries[key] = { ...(current ?? blank()), revalidating: true };

    const result = await commands.cmdLingqCourse(lang, collectionId);

    if (result.status === "ok") {
      entries[key] = {
        view: result.data,
        fetchedAt: Date.now(),
        revalidating: false,
        error: null,
      };
    } else {
      entries[key] = {
        ...entries[key],
        revalidating: false,
        error: result.error,
      };
    }
  },
};
