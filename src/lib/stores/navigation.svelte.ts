import { goto } from "$app/navigation";

// App-created history, not the browser's: `history.length` also counts
// entries from outside our control (initial load, hash changes SvelteKit
// makes internally), so Back/Forward availability is tracked from scratch
// here instead.
let entries = $state<string[]>([]);
let index = $state(-1);

// Set to the target URL right before goBack/goForward replace it, so the
// matching afterNavigate → record() call is a no-op instead of appending a
// new entry. Compared against the recorded URL (not just consumed blindly)
// so a navigation that interleaves before ours completes isn't swallowed,
// and cleared after the await in case ours never fires at all (an aborted
// or superseded goto) so a stale suppression can't eat a later navigation.
let suppressedUrl: string | null = null;

// Set by replace() for the navigation it starts. Deliberately not $state: it
// is assigned from inside a caller's effect, and reactive bookkeeping there
// would feed back into the same flush.
let replacedUrl: string | null = null;

function canGoBack(): boolean {
  return index > 0;
}

function canGoForward(): boolean {
  return index < entries.length - 1;
}

async function travel(next: number): Promise<void> {
  index = next;
  suppressedUrl = entries[next];
  await goto(entries[next], { replaceState: true });
  suppressedUrl = null;
}

export const navigationHistory = {
  get canGoBack(): boolean {
    return canGoBack();
  },
  get canGoForward(): boolean {
    return canGoForward();
  },
  record(url: string): void {
    if (suppressedUrl === url) {
      suppressedUrl = null;
      return;
    }
    if (entries[index] === url) return;
    if (replacedUrl === url && index >= 0) {
      replacedUrl = null;
      entries = [...entries.slice(0, index), url, ...entries.slice(index + 1)];
      return;
    }
    entries = [...entries.slice(0, index + 1), url];
    index = entries.length - 1;
  },
  // Navigates without deepening the stack: the destination takes the current
  // entry's place, so Back still leads out of the route rather than back
  // through every intermediate URL a live filter wrote.
  async replace(url: string): Promise<void> {
    replacedUrl = url;
    await goto(url, { replaceState: true, keepFocus: true, noScroll: true });
    replacedUrl = null;
  },
  async goBack(): Promise<void> {
    if (canGoBack()) await travel(index - 1);
  },
  async goForward(): Promise<void> {
    if (canGoForward()) await travel(index + 1);
  },
};
