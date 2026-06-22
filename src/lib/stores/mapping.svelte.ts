import {
  commands,
  type AbsorbPolicy,
  type BucketMeta,
  type ChapterId,
  type ChapterMeta,
  type MappingOp,
  type MappingState,
  type ProjectId,
} from "$lib/ipc/bindings";
import { assetUrl } from "$lib/audio";

type State = {
  projectId: ProjectId | null;
  chapters: ChapterMeta[];
  skippedIds: ChapterId[];
  mappingState: MappingState | null;
  absorbPolicy: AbsorbPolicy;
  status: "idle" | "loading" | "ready" | "error";
  error: string | null;
  // Ticks whenever an optimistic update is reverted by a backend error.
  // Consumers can subscribe to re-align local row state from skippedIds
  // without re-seeding on first mount. AD-025: revert is silent.
  revertEpoch: number;
  // Wall-clock ms of the last successful save (selection or mapping op).
  // Footer renders "All changes saved · {relative time}" off this.
  lastSavedAt: number | null;
  // Writes queued or in flight. Footer renders "Saving…" while > 0.
  pendingWrites: number;
  selectedChapterId: ChapterId | null;
  chapterTextCache: Record<string, string>;
};

const state = $state<State>({
  projectId: null,
  chapters: [],
  skippedIds: [],
  mappingState: null,
  absorbPolicy: "forward",
  status: "idle",
  error: null,
  revertEpoch: 0,
  lastSavedAt: null,
  pendingWrites: 0,
  selectedChapterId: null,
  chapterTextCache: {},
});

// Serialise concurrent setSkipped/submitOp calls so the backend always sees
// the same arrival order the user produced. Without this, two rapid toggles
// can race and the older write can win.
let pendingFlush: Promise<void> = Promise.resolve();

let opDebounceTimer: ReturnType<typeof setTimeout> | null = null;
// Wall-clock of the first enqueue in the current debounce window. Used to
// cap how long a rapid burst can starve the flush: once 500ms has elapsed
// since the first op, the next op flushes immediately instead of resetting
// the timer again.
let firstEnqueueAt: number | null = null;
const OP_DEBOUNCE_MS = 500;
const OP_MAX_WAIT_MS = 500;
type QueuedOp = {
  op: MappingOp;
  // mappingState captured at enqueue time, before any optimistic mutation
  // for this op landed. Revert target on backend rejection.
  snapshot: MappingState | null;
  resolve: () => void;
  reject: (err: unknown) => void;
};
let pendingOps: QueuedOp[] = [];

function settle(q: QueuedOp, err?: unknown) {
  state.pendingWrites = Math.max(0, state.pendingWrites - 1);
  if (err === undefined) q.resolve();
  else q.reject(err);
}

function drainQueue() {
  if (opDebounceTimer != null) {
    clearTimeout(opDebounceTimer);
    opDebounceTimer = null;
  }
  firstEnqueueAt = null;
  for (const q of pendingOps.splice(0)) settle(q);
}

function labelForTrack(m: MappingState, trackId: string): string {
  const idx = (m.buckets ?? []).findIndex((b) => b.trackId === trackId);
  return `Audio ${idx >= 0 ? idx + 1 : "?"}`;
}

function scoreGate(s: MappingState | null): boolean {
  if (!s) return true;
  // Track-less pairs are excluded: there is no persistable op to confirm
  // them, so they must not gate Continue. The gate blocks on the ORIGINAL
  // score — an untouched pair stays red even if `confidence` was bumped by
  // a displacing op.
  return s.pairs.every(
    (p) =>
      !p.track_id ||
      (p.touched ?? false) ||
      (p.original_confidence ?? p.confidence) >= 0.6,
  );
}

async function flushPendingOps(): Promise<void> {
  if (pendingOps.length === 0) return;
  if (!state.projectId) {
    for (const q of pendingOps.splice(0)) settle(q);
    return;
  }
  const projectId = state.projectId;
  const ops = pendingOps;
  pendingOps = [];
  // Backend-confirmed state from earlier ops in this batch. A later failure
  // reverts here rather than to its own snapshot, which would resurrect the
  // optimism of already-rejected siblings.
  let lastConfirmed: MappingState | null = null;
  for (let i = 0; i < ops.length; i++) {
    const queued = ops[i];
    const current = state.mappingState;
    if (!current || state.projectId !== projectId) {
      for (let j = i; j < ops.length; j++) settle(ops[j]);
      return;
    }
    const expected = (current.op_id ?? 0) + 1;
    const result = await commands.cmdApplyMappingOp(projectId, queued.op, expected);
    const stillCurrent = state.projectId === projectId;
    if (result.status === "error") {
      if (stillCurrent) {
        // AD-025: silent revert.
        state.mappingState = lastConfirmed ?? queued.snapshot;
        state.revertEpoch += 1;
      }
      // eslint-disable-next-line no-console
      console.warn("cmd_apply_mapping_op failed; reverted optimistic update");
      const err = result.error;
      settle(queued, err);
      for (let j = i + 1; j < ops.length; j++) settle(ops[j], err);
      return;
    }
    if (stillCurrent) {
      state.mappingState = result.data;
      state.lastSavedAt = Date.now();
    }
    lastConfirmed = result.data;
    settle(queued);
  }
}

export const mapping = {
  get projectId() {
    return state.projectId;
  },
  get chapters() {
    return state.chapters;
  },
  get skippedIds() {
    return state.skippedIds;
  },
  get mappingState() {
    return state.mappingState;
  },
  get absorbPolicy() {
    return state.absorbPolicy;
  },
  get status() {
    return state.status;
  },
  get error() {
    return state.error;
  },
  get revertEpoch() {
    return state.revertEpoch;
  },
  get lastSavedAt() {
    return state.lastSavedAt;
  },
  get saving() {
    return state.pendingWrites > 0;
  },
  get buckets(): BucketMeta[] {
    return state.mappingState?.buckets ?? [];
  },

  async load(key: string) {
    drainQueue();
    state.pendingWrites = 0;
    state.status = "loading";
    state.error = null;
    // Clear project-scoped state synchronously so a reused match-page
    // instance never renders the previous project's chapters/mapping
    // during the in-flight load.
    state.projectId = null;
    state.chapters = [];
    state.skippedIds = [];
    state.mappingState = null;
    state.absorbPolicy = "forward";
    state.revertEpoch = 0;
    state.selectedChapterId = null;
    state.chapterTextCache = {};
    state.lastSavedAt = null;
    const loaded = await commands.cmdProjectLoad(key);
    if (loaded.status === "error") {
      state.status = "error";
      state.error = "Failed to load project";
      return;
    }
    state.projectId = loaded.data.id;
    state.skippedIds = loaded.data.skipped_chapters ?? [];
    state.mappingState = loaded.data.mapping ?? null;
    state.absorbPolicy = loaded.data.absorb_policy ?? "forward";

    const chaptersResult = await commands.cmdProjectChapters(loaded.data.id);
    if (chaptersResult.status === "error") {
      state.status = "error";
      state.error = "Failed to load chapters";
      return;
    }
    state.chapters = chaptersResult.data;
    state.status = "ready";
  },

  /** Test-only seed for the mapping editor — bypasses the parse round trip. */
  seedMapping(next: MappingState) {
    state.mappingState = next;
  },

  setSkipped(skippedIds: ChapterId[]): Promise<void> {
    if (!state.projectId) return Promise.resolve();
    const projectId = state.projectId;
    const previous = [...state.skippedIds];
    state.skippedIds = skippedIds;
    state.pendingWrites += 1;
    const run = async () => {
      try {
        const result = await commands.cmdSetSelection(projectId, skippedIds);
        if (state.projectId !== projectId) return;
        if (result.status === "error") {
          // AD-025: silent revert. Roll back optimistic state and bump
          // revertEpoch so consumers can re-align row state.
          state.skippedIds = previous;
          state.revertEpoch += 1;
          // eslint-disable-next-line no-console
          console.warn("cmd_set_selection failed; reverted optimistic update");
          return;
        }
        state.lastSavedAt = Date.now();
      } finally {
        state.pendingWrites = Math.max(0, state.pendingWrites - 1);
      }
    };
    pendingFlush = pendingFlush.then(run, run);
    return pendingFlush;
  },

  /**
   * Queue a mapping op. Debounced 500ms — concurrent ops during the window
   * coalesce into a single flush invocation that drains them in order. The
   * returned promise resolves only after the IPC for this specific op
   * settles successfully, or rejects when its flush turn fails.
   *
   * `snapshot` is the pre-optimism mappingState; callers that mutate state
   * optimistically before enqueueing must pass the state they started from.
   */
  submitOp(op: MappingOp, snapshot?: MappingState | null): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      pendingOps.push({
        op,
        snapshot: snapshot !== undefined ? snapshot : state.mappingState,
        resolve,
        reject,
      });
      state.pendingWrites += 1;
      const now = Date.now();
      if (firstEnqueueAt == null) firstEnqueueAt = now;
      // Max-wait cap: if the first op in this burst is already older than
      // the debounce window, flush now instead of resetting the timer.
      if (now - firstEnqueueAt >= OP_MAX_WAIT_MS) {
        if (opDebounceTimer != null) {
          clearTimeout(opDebounceTimer);
          opDebounceTimer = null;
        }
        firstEnqueueAt = null;
        pendingFlush = pendingFlush.then(flushPendingOps, flushPendingOps);
        return;
      }
      if (opDebounceTimer != null) clearTimeout(opDebounceTimer);
      opDebounceTimer = setTimeout(() => {
        opDebounceTimer = null;
        firstEnqueueAt = null;
        pendingFlush = pendingFlush.then(flushPendingOps, flushPendingOps);
      }, OP_DEBOUNCE_MS);
    });
  },

  /** Mark a pair as touched without changing the assignment (Confirm click). */
  confirmPair(chapterId: ChapterId): Promise<void> {
    const cur = state.mappingState;
    if (!cur) return Promise.resolve();
    const idx = cur.pairs.findIndex((p) => p.chapter_id === chapterId);
    if (idx < 0) return Promise.resolve();
    const trackId = cur.pairs[idx].track_id;
    // Track-less pairs have no persistable touch op and are excluded from
    // the score gate, so confirming them is a no-op.
    if (!trackId) return Promise.resolve();
    // Optimistically mark touched so the score gate clears immediately —
    // the debounced submitOp catches the server up within 500ms.
    const nextPairs = [...cur.pairs];
    nextPairs[idx] = { ...nextPairs[idx], touched: true };
    state.mappingState = { ...cur, pairs: nextPairs };
    return this.submitOp(
      { kind: "swap", chapter_id: chapterId, track_id: trackId },
      cur,
    );
  },

  // Remove a chapter from the upload set: add it to the skip set. The upload
  // plan already excludes skipped chapters (it re-packs over the eligible
  // set), so a plain selection write is the whole operation — no re-split.
  // Undone via the Removed strip (setSkipped without the chapter).
  removeChapter(chapterId: ChapterId): Promise<void> {
    return this.setSkipped([...state.skippedIds, chapterId]);
  },

  moveChapter(chapterId: ChapterId, trackId: string): Promise<void> {
    return this.submitOp({ kind: "reassign", chapter_id: chapterId, track_id: trackId });
  },

  // An edge chapter of its bucket can move to the neighbouring bucket only,
  // preserving the contiguous-bucket invariant.
  adjacentTracksFor(chapterId: ChapterId): { trackId: string; label: string }[] {
    const m = state.mappingState;
    if (!m) return [];
    const ordered = state.chapters
      .filter((c) => !state.skippedIds.includes(c.id))
      .map((c) => m.pairs.find((p) => p.chapter_id === c.id))
      .filter((p): p is NonNullable<typeof p> => !!p && !!p.track_id);
    const i = ordered.findIndex((p) => p.chapter_id === chapterId);
    if (i < 0) return [];
    const self = ordered[i].track_id;
    const out: { trackId: string; label: string }[] = [];
    const prev = ordered[i - 1];
    const next = ordered[i + 1];
    // first-of-bucket: prev belongs to a different (earlier) bucket
    if (prev && prev.track_id !== self) out.push({ trackId: prev.track_id!, label: labelForTrack(m, prev.track_id!) });
    // last-of-bucket: next belongs to a different (later) bucket
    if (next && next.track_id !== self) out.push({ trackId: next.track_id!, label: labelForTrack(m, next.track_id!) });
    return out;
  },

  get selectedChapterId() {
    return state.selectedChapterId;
  },
  selectChapter(id: ChapterId) {
    state.selectedChapterId = id;
    if (state.chapterTextCache[id] === undefined) {
      void commands.cmdChapterText(state.projectId!, id).then((res) => {
        if (res.status === "ok") state.chapterTextCache = { ...state.chapterTextCache, [id]: res.data };
      });
    }
  },
  chapterTextFor(id: ChapterId): string | null {
    return state.chapterTextCache[id] ?? null;
  },

  selectedBucketAudio(): { src: string; start: number; end: number } | null {
    const id = state.selectedChapterId;
    if (!id || !state.mappingState) return null;
    const pair = state.mappingState.pairs.find((p) => p.chapter_id === id);
    if (!pair?.track_id) return null;
    const bucket = (state.mappingState.buckets ?? []).find(
      (b) => b.trackId === pair.track_id,
    );
    if (!bucket?.audioPath) return null;
    const [start, end] = bucket.window ?? [0, bucket.atomDurationSec];
    return { src: assetUrl(bucket.audioPath), start, end };
  },

  gateContinue(): boolean {
    return scoreGate(state.mappingState);
  },

  /** Await the tail of the in-flight write queue. */
  flush(): Promise<void> {
    if (opDebounceTimer != null) {
      clearTimeout(opDebounceTimer);
      opDebounceTimer = null;
      firstEnqueueAt = null;
      pendingFlush = pendingFlush.then(flushPendingOps, flushPendingOps);
    }
    return pendingFlush;
  },

  reset() {
    state.projectId = null;
    state.chapters = [];
    state.skippedIds = [];
    state.mappingState = null;
    state.absorbPolicy = "forward";
    state.status = "idle";
    state.error = null;
    state.revertEpoch = 0;
    state.lastSavedAt = null;
    state.selectedChapterId = null;
    state.chapterTextCache = {};
    drainQueue();
    state.pendingWrites = 0;
  },
};
