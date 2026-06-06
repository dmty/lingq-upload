import { commands, type LibraryIndex, type AppError } from "$lib/ipc/bindings";

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
  },
};
