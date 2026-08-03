import { untrack } from "svelte";
import {
  commands,
  type AppError,
  type CourseView,
} from "$lib/ipc/bindings";
import { isLingqNotFound } from "$lib/errors";

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

    // Suppression exists for the come-and-go case: bouncing between the
    // library and a course should not re-hit LingQ each time.
    const age = current?.fetchedAt == null ? Infinity : Date.now() - current.fetchedAt;
    if (!force && current?.view != null && age < STALE_AFTER_MS) return;

    entries[key] = { ...(current ?? blank()), revalidating: true };

    // The generated binding rethrows genuine `Error` instances instead of
    // returning them as a result (transport-level failures, not app-level
    // ones). Without this catch, a throw here would leave `revalidating`
    // stuck `true` forever — the mount effect and Refresh button both skip
    // an entry that's already revalidating, so the screen would wedge with
    // no way to retry short of restarting the app.
    try {
      const result = await commands.cmdLingqCourse(lang, collectionId);

      if (result.status === "ok") {
        entries[key] = {
          view: result.data,
          fetchedAt: Date.now(),
          revalidating: false,
          error: null,
        };
      } else {
        const notFound = isLingqNotFound(result.error);
        entries[key] = {
          // A course deleted on LingQ makes cached stats wrong, not stale.
          view: notFound ? null : entries[key].view,
          fetchedAt: notFound ? null : entries[key].fetchedAt,
          revalidating: false,
          error: result.error,
        };
      }
    } catch (e) {
      entries[key] = {
        ...entries[key],
        revalidating: false,
        error: { kind: "Other", message: e instanceof Error ? e.message : String(e) },
      };
    }
  },
};
