export function formatCount(n: number | null): string {
  return n == null ? "—" : n.toLocaleString();
}

export function formatRelative(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const now = new Date();
  const startOfDay = (x: Date) =>
    new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const dayDiff = Math.floor((startOfDay(now) - startOfDay(d)) / 86_400_000);
  if (dayDiff <= 0) return "today";
  if (dayDiff === 1) return "yesterday";
  if (dayDiff < 7) {
    return new Intl.DateTimeFormat(undefined, { weekday: "short" }).format(d);
  }
  if (d.getFullYear() === now.getFullYear()) {
    return new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
    }).format(d);
  }
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  }).format(d);
}

// Minute-granularity age, for "how stale is this number" labels.
// `formatRelative` is day-granularity and would render every fresh fetch as
// "today", which is useless for a cache-freshness indicator.
export function formatAge(iso: string, now: number = Date.now()): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return "unknown";
  const minutes = Math.floor((now - t) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes === 1) return "1 minute ago";
  if (minutes < 60) return `${minutes} minutes ago`;
  const hours = Math.floor(minutes / 60);
  if (hours === 1) return "1 hour ago";
  if (hours < 24) return `${hours} hours ago`;
  return formatRelative(iso);
}

// CJK characters and the fullwidth punctuation that travels with them.
// Deliberately excludes U+3000 (ideographic space), which is a separator.
const CJK_SOURCE =
  "[\\u3001-\\u303f\\u3040-\\u30ff\\u3400-\\u4dbf\\u4e00-\\u9fff\\uf900-\\ufaff\\uff01-\\uff60\\uff66-\\uff9f]";
const CJK = new RegExp(CJK_SOURCE);
const CJK_GAP = new RegExp(`(${CJK_SOURCE})[ \\u3000]+(?=${CJK_SOURCE})`, "g");

/**
 * LingQ returns lesson titles as its tokenizer left them: the lesson number
 * fused to the front and a space between every CJK token. The row carries its
 * own index, so the number goes, and the spaces were never in the book.
 */
export function normalizeLessonTitle(title: string): string {
  let out = title.trim();
  // Only a number running straight into CJK text is the fused lesson number —
  // a title that is genuinely numeric ("1984") is left alone.
  const fused = out.match(/^\d+(?::\d+)?(?=\S)/);
  if (fused && CJK.test(out.charAt(fused[0].length))) {
    out = out.slice(fused[0].length);
  }
  return out.replace(CJK_GAP, "$1").trim();
}

// Clock duration for a single track or position: m:ss under an hour,
// h:mm:ss at or above it. Anything longer written as m:ss ("116:00") reads
// as a bug rather than a duration.
export function formatClock(sec: number): string {
  const t = Math.max(0, Math.round(sec));
  const s = t % 60;
  const m = Math.floor(t / 60) % 60;
  const h = Math.floor(t / 3600);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

// Coarse duration for totals ("2h 45m"). Per-track labels use formatClock;
// a book-length total in m:ss is unreadable.
export function formatDuration(sec: number): string {
  const t = Math.max(0, Math.round(sec));
  if (t < 60) return `${t}s`;
  const m = Math.floor(t / 60);
  if (m < 60) return `${m}m`;
  return `${Math.floor(m / 60)}h ${(m % 60).toString().padStart(2, "0")}m`;
}
