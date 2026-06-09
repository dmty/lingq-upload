import {
  commands,
  type Chapter,
  type ChapterId,
  type ProjectId,
} from "$lib/ipc/bindings";

type State = {
  projectId: ProjectId | null;
  chapters: Chapter[];
  skippedIds: ChapterId[];
  status: "idle" | "loading" | "ready" | "error";
  error: string | null;
};

const state = $state<State>({
  projectId: null,
  chapters: [],
  skippedIds: [],
  status: "idle",
  error: null,
});

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

  async setSkipped(skippedIds: ChapterId[]) {
    if (!state.projectId) return;
    state.skippedIds = skippedIds;
    const result = await commands.cmdSetSelection(state.projectId, skippedIds);
    if (result.status === "error") {
      state.error = "Failed to save selection";
    }
  },

  reset() {
    state.projectId = null;
    state.chapters = [];
    state.skippedIds = [];
    state.status = "idle";
    state.error = null;
  },
};
