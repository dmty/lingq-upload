export const SIDEBAR_STORAGE_KEY = "sidebar:v1";
export const SIDEBAR_MIN_WIDTH = 180;
export const SIDEBAR_MAX_WIDTH = 400;
export const SIDEBAR_DEFAULT_WIDTH = 220;

type Persisted = { width: number; collapsed: boolean };

function clampWidth(px: number): number {
  if (!Number.isFinite(px)) return SIDEBAR_DEFAULT_WIDTH;
  return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, Math.round(px)));
}

function readPersisted(): Persisted {
  const fallback: Persisted = {
    width: SIDEBAR_DEFAULT_WIDTH,
    collapsed: false,
  };
  if (typeof window === "undefined") return fallback;
  try {
    const raw = window.localStorage.getItem(SIDEBAR_STORAGE_KEY);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as Partial<Persisted>;
    return {
      width: clampWidth(parsed.width ?? SIDEBAR_DEFAULT_WIDTH),
      collapsed: Boolean(parsed.collapsed),
    };
  } catch {
    return fallback;
  }
}

function writePersisted(next: Persisted): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(SIDEBAR_STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Quota or disabled storage — silently drop; state stays in memory.
  }
}

export class SidebarState {
  width = $state(SIDEBAR_DEFAULT_WIDTH);
  collapsed = $state(false);

  constructor() {
    const initial = readPersisted();
    this.width = initial.width;
    this.collapsed = initial.collapsed;
  }

  toggle(): void {
    this.collapsed = !this.collapsed;
    this.persist();
  }

  setWidth(px: number): void {
    const next = clampWidth(px);
    if (next === this.width) return;
    this.width = next;
    this.persist();
  }

  private persist(): void {
    writePersisted({ width: this.width, collapsed: this.collapsed });
  }
}

export const sidebar = new SidebarState();
