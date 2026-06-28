import {
  commands,
  type LibraryIndex,
  type AppError,
  type ProjectId,
} from "$lib/ipc/bindings";
import { joinKey } from "$lib/identity";

type State = {
  status: "idle" | "loading" | "ready" | "error";
  index: LibraryIndex | null;
  error: AppError | null;
};

const state = $state<State>({
  status: "idle",
  index: null,
  error: null,
});

let inflight = 0;

export const library = {
  get status() {
    return state.status;
  },
  get index() {
    return state.index;
  },
  get error() {
    return state.error;
  },
  async load() {
    const ticket = ++inflight;
    state.status = "loading";
    try {
      const result = await commands.cmdLibraryList();
      if (ticket !== inflight) return;
      if (result.status === "ok") {
        state.index = result.data;
        state.error = null;
        state.status = "ready";
      } else {
        state.error = result.error;
        state.status = "error";
      }
    } catch (e) {
      if (ticket !== inflight) return;
      state.error = {
        kind: "Other",
        message: e instanceof Error ? e.message : String(e),
      };
      state.status = "error";
    }
  },
  removeById(id: ProjectId) {
    if (!state.index) return;
    const target = joinKey(id);
    state.index.entries = state.index.entries.filter(
      (e) => joinKey(e.id) !== target,
    );
  },
};
