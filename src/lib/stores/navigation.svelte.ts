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

function canGoBack(): boolean {
  return index > 0;
}

function canGoForward(): boolean {
  return index >= 0 && index < entries.length - 1;
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
    entries = [...entries.slice(0, index + 1), url];
    index = entries.length - 1;
  },
  async goBack(): Promise<void> {
    if (!canGoBack()) return;
    index -= 1;
    suppressedUrl = entries[index];
    await goto(entries[index], { replaceState: true });
    suppressedUrl = null;
  },
  async goForward(): Promise<void> {
    if (!canGoForward()) return;
    index += 1;
    suppressedUrl = entries[index];
    await goto(entries[index], { replaceState: true });
    suppressedUrl = null;
  },
};
