import { commands, type Language } from "$lib/ipc/bindings";
import { appErrorMessage } from "$lib/errors";

const STORAGE_KEY = "cache.languages.v1";
const SELECTION_KEY = "addProject.lang";

export function formatLanguageOption(l: Language): string {
  return l.known_words > 0
    ? `${l.title} (${l.known_words.toLocaleString()})`
    : l.title;
}

export function getSavedLanguage(): string {
  return localStorage.getItem(SELECTION_KEY) ?? "";
}

export function saveLanguage(code: string) {
  localStorage.setItem(SELECTION_KEY, code);
}

export function clearSavedLanguage() {
  localStorage.removeItem(SELECTION_KEY);
}

type Snapshot = {
  username: string | null;
  languages: Language[];
  fetchedAt: number;
};

function readSnapshot(): Snapshot | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Snapshot;
    if (!Array.isArray(parsed.languages)) return null;
    return parsed;
  } catch {
    return null;
  }
}

function writeSnapshot(snap: Snapshot) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(snap));
  } catch {
    // Quota/private mode — silently skip; cache is best-effort.
  }
}

class LanguagesStore {
  languages = $state<Language[]>([]);
  username = $state<string | null>(null);
  error = $state<string | null>(null);
  loaded = $state(false);
  refreshing = $state(false);

  private refreshPromise: Promise<void> | null = null;

  constructor() {
    const snap = readSnapshot();
    if (snap) {
      this.languages = snap.languages;
      this.username = snap.username;
      this.loaded = true;
    }
  }

  // Idempotent per session — first call kicks off the background refresh;
  // subsequent callers await the same promise. Cached snapshot stays visible
  // until the network call resolves.
  ensureLoaded(): Promise<void> {
    if (this.refreshPromise) return this.refreshPromise;
    const p = this.refresh().finally(() => {
      // Successful refreshes are cached for the session; a failed refresh
      // releases the slot so the next caller can retry without an app restart.
      if (this.error !== null) this.refreshPromise = null;
    });
    this.refreshPromise = p;
    return p;
  }

  private async refresh(): Promise<void> {
    this.refreshing = true;
    let username: string | null = null;
    const profileRes = await commands.cmdAccountProfile();
    if (profileRes.status === "ok") {
      username = profileRes.data.username;
    }
    const res = await commands.cmdListLanguages(username);
    if (res.status === "ok") {
      const sorted = [...res.data].sort(
        (a, b) => b.known_words - a.known_words,
      );
      this.languages = sorted;
      this.username = username;
      this.loaded = true;
      this.error = null;
      writeSnapshot({
        username,
        languages: sorted,
        fetchedAt: Date.now(),
      });
    } else {
      this.error = appErrorMessage(res.error);
    }
    this.refreshing = false;
  }

  // Logout / key change — drop everything and force the next ensureLoaded
  // to fetch from scratch.
  invalidate() {
    this.languages = [];
    this.username = null;
    this.error = null;
    this.loaded = false;
    this.refreshPromise = null;
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // ignore
    }
  }
}

export const languagesStore = new LanguagesStore();
