// Tauri IPC stub for Playwright runs against Vite.
//
// The real app reaches `window.__TAURI_INTERNALS__.invoke`/`event` to talk to
// the Rust backend. Vite-only `bun run dev` boots without that runtime, so
// every component that calls a generated `commands.*` binding throws on
// mount. We inject a minimal in-page shim before the page script runs so the
// empty-state smoke can render without a real Tauri webview.
//
// Only the commands the empty-state path actually invokes need to be
// stubbed. Add more entries as the smoke surface grows.

export const tauriStubInitScript = `
;(() => {
    if (window.__TAURI_INTERNALS__) return;

    const handlers = {
        // Library list returns an empty index so the empty-state CTA renders.
        cmd_library_list: () => ({ projects: [] }),
        // Account/profile probe — empty placeholder keeps the layout calm.
        cmd_account_profile: () => ({ username: null, known_words: {} }),
    };

    window.__TAURI_INTERNALS__ = {
        invoke: async (cmd, _args) => {
            const fn = handlers[cmd];
            if (!fn) {
                throw new Error('tauri-stub: unmocked command ' + cmd);
            }
            return fn();
        },
        transformCallback: (cb) => {
            const id = Math.floor(Math.random() * 2 ** 31);
            window['_' + id] = cb;
            return id;
        },
    };
})();
`;
