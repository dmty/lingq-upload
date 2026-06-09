import {
  commands,
  type ChapterId,
  type ChapterMeta,
  type MappingOp,
  type MappingState,
  type ProjectId,
} from "$lib/ipc/bindings";

type State = {
  projectId: ProjectId | null;
  chapters: ChapterMeta[];
  skippedIds: ChapterId[];
  mappingState: MappingState | null;
  status: "idle" | "loading" | "ready" | "error";
  error: string | null;
  // Ticks whenever an optimistic update is reverted by a backend error.
  // Consumers can subscribe to re-align local row state from skippedIds
  // without re-seeding on first mount. AD-025: revert is silent.
  revertEpoch: number;
  // Wall-clock ms of the last successful save (selection or mapping op).
  // Footer renders "All changes saved · {relative time}" off this.
  lastSavedAt: number | null;
};

const state = $state<State>({
  projectId: null,
  chapters: [],
  skippedIds: [],
  mappingState: null,
  status: "idle",
  error: null,
  revertEpoch: 0,
  lastSavedAt: null,
});

// Serialise concurrent setSkipped/submitOp calls so the backend always sees
// the same arrival order the user produced. Without this, two rapid toggles
// can race and the older write can win.
let pendingFlush: Promise<void> = Promise.resolve();

let opDebounceTimer: ReturnType<typeof setTimeout> | null = null;
type QueuedOp = {
  op: MappingOp;
  resolve: () => void;
  reject: (err: unknown) => void;
};
let pendingOps: QueuedOp[] = [];

function scoreGate(s: MappingState | null): boolean {
  if (!s) return true;
  return s.pairs.every((p) => (p.touched ?? false) || p.confidence >= 0.6);
}

async function flushPendingOps(): Promise<void> {
  if (pendingOps.length === 0) return;
  if (!state.projectId) {
    const queued = pendingOps;
    pendingOps = [];
    for (const q of queued) q.resolve();
    return;
  }
  const projectId = state.projectId;
  const ops = pendingOps;
  pendingOps = [];
  for (let i = 0; i < ops.length; i++) {
    const queued = ops[i];
    const current = state.mappingState;
    if (!current) {
      for (let j = i; j < ops.length; j++) ops[j].resolve();
      break;
    }
    const prevSnapshot = current;
    const expected = (current.op_id ?? 0) + 1;
    const result = await commands.cmdApplyMappingOp(projectId, queued.op, expected);
    if (result.status === "error") {
      // AD-025: silent revert.
      state.mappingState = prevSnapshot;
      state.revertEpoch += 1;
      // eslint-disable-next-line no-console
      console.warn("cmd_apply_mapping_op failed; reverted optimistic update");
      const err = result.error;
      queued.reject(err);
      for (let j = i + 1; j < ops.length; j++) ops[j].reject(err);
      return;
    }
    state.mappingState = result.data;
    state.lastSavedAt = Date.now();
    queued.resolve();
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

  async load(key: string) {
    state.status = "loading";
    state.error = null;
    const loaded = await commands.cmdProjectLoad(key);
    if (loaded.status === "error") {
      state.status = "error";
      state.error = "Failed to load project";
      return;
    }
    state.projectId = loaded.data.id;
    state.skippedIds = loaded.data.skipped_chapters ?? [];
    state.mappingState = loaded.data.mapping ?? null;

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
    const run = async () => {
      const result = await commands.cmdSetSelection(projectId, skippedIds);
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
    };
    pendingFlush = pendingFlush.then(run, run);
    return pendingFlush;
  },

  /**
   * Queue a mapping op. Debounced 500ms — concurrent ops during the window
   * coalesce into a single flush invocation that drains them in order. The
   * returned promise resolves only after the IPC for this specific op
   * settles successfully, or rejects when its flush turn fails.
   */
  submitOp(op: MappingOp): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      pendingOps.push({ op, resolve, reject });
      if (opDebounceTimer != null) clearTimeout(opDebounceTimer);
      opDebounceTimer = setTimeout(() => {
        opDebounceTimer = null;
        pendingFlush = pendingFlush.then(flushPendingOps, flushPendingOps);
      }, 500);
    });
  },

  /** Mark a pair as touched without changing the assignment (Confirm click). */
  confirmPair(chapterId: ChapterId): Promise<void> {
    const cur = state.mappingState;
    if (!cur) return Promise.resolve();
    const idx = cur.pairs.findIndex((p) => p.chapter_id === chapterId);
    if (idx < 0) return Promise.resolve();
    // Optimistically mark touched so the score gate clears immediately —
    // the debounced submitOp catches the server up within 500ms.
    const nextPairs = [...cur.pairs];
    nextPairs[idx] = { ...nextPairs[idx], touched: true };
    state.mappingState = { ...cur, pairs: nextPairs };
    const trackId = cur.pairs[idx].track_id;
    if (trackId) {
      return this.submitOp({
        kind: "swap",
        chapter_id: chapterId,
        track_id: trackId,
      });
    }
    return Promise.resolve();
  },

  gateContinue(): boolean {
    return scoreGate(state.mappingState);
  },

  /** Await the tail of the in-flight write queue. */
  flush(): Promise<void> {
    if (opDebounceTimer != null) {
      clearTimeout(opDebounceTimer);
      opDebounceTimer = null;
      pendingFlush = pendingFlush.then(flushPendingOps, flushPendingOps);
    }
    return pendingFlush;
  },

  reset() {
    state.projectId = null;
    state.chapters = [];
    state.skippedIds = [];
    state.mappingState = null;
    state.status = "idle";
    state.error = null;
    state.revertEpoch = 0;
    state.lastSavedAt = null;
    const drained = pendingOps;
    pendingOps = [];
    for (const q of drained) q.resolve();
    if (opDebounceTimer != null) {
      clearTimeout(opDebounceTimer);
      opDebounceTimer = null;
    }
  },
};
