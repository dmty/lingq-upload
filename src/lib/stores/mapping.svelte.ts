import {
  commands,
  type ChapterId,
  type ChapterMeta,
  type ProjectId,
} from "$lib/ipc/bindings";

type State = {
  projectId: ProjectId | null;
  chapters: ChapterMeta[];
  skippedIds: ChapterId[];
  status: "idle" | "loading" | "ready" | "error";
  error: string | null;
  // Ticks whenever an optimistic update is reverted by a backend error.
  // Consumers can subscribe to re-align local row state from skippedIds
  // without re-seeding on first mount. AD-025: revert is silent.
  revertEpoch: number;
};

const state = $state<State>({
  projectId: null,
  chapters: [],
  skippedIds: [],
  status: "idle",
  error: null,
  revertEpoch: 0,
});

// Serialise concurrent setSkipped calls so the backend always sees the same
// arrival order the user produced. Without this, two rapid toggles can race
// and the older write can win.
let pendingFlush: Promise<void> = Promise.resolve();

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
  get status() {
    return state.status;
  },
  get error() {
    return state.error;
  },
  get revertEpoch() {
    return state.revertEpoch;
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

    const chaptersResult = await commands.cmdProjectChapters(loaded.data.id);
    if (chaptersResult.status === "error") {
      state.status = "error";
      state.error = "Failed to load chapters";
      return;
    }
    state.chapters = chaptersResult.data;
    state.status = "ready";
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
      }
    };
    pendingFlush = pendingFlush.then(run, run);
    return pendingFlush;
  },

  /** Await the tail of the in-flight write queue. */
  flush(): Promise<void> {
    return pendingFlush;
  },

  reset() {
    state.projectId = null;
    state.chapters = [];
    state.skippedIds = [];
    state.status = "idle";
    state.error = null;
    state.revertEpoch = 0;
  },
};
