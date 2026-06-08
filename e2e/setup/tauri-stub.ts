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
        cmd_library_list: () => ({
            schema_version: 1,
            generated_at: new Date().toISOString(),
            entries: [],
        }),
        // Account/profile probe — empty placeholder keeps the layout calm.
        cmd_account_profile: () => ({ username: null, known_words: {} }),
        // No LingQ key in the test env — surfaces the dismissable banner,
        // never any recovery-event copy.
        cmd_load_lingq_key: () => null,
        // Project load for the run screen. Returns a minimal project with
        // no receipts so the chapter list renders empty and the start
        // button shows "Start" (not "Resume" — which would tax the
        // forbidden-word filter on /run).
        cmd_project_load: (args) => ({
            schema_version: 1,
            id: {
                content_hash: (args && args.key) || "stub-project",
                audible_asin: null,
                isbn13: null,
                calibre_uuid: null,
            },
            sources: { text: null, audio: null },
            settings: {
                language: "en",
                collection_title: "Stub Project",
                level: 1,
                tags: [],
            },
            receipts: [],
            queue_cursor: 0,
            completed_lesson_ids: [],
            matcher_decision: null,
            cover_path: null,
            authors: [],
            series: null,
            lingq_collection_id: null,
            last_activity_at: null,
            stage: "mapped",
            last_transition_at: null,
        }),
        // Mismatch inspection for the /match route. None = no decision yet,
        // route renders the empty resolver shell without any decision copy.
        cmd_matcher_inspect: () => null,
        // Trash list for the /settings route. Empty list keeps the panel quiet.
        cmd_list_trash: () => [],
        // Event plugin: register a listener, return a numeric id. We don't
        // emit anything from the stub yet — specs that need driven events
        // stay skipped until this grows.
        "plugin:event|listen": () => 1,
        "plugin:event|unlisten": () => null,
    };

    window.__TAURI_INTERNALS__ = {
        // Webview/window metadata so getCurrentWebview()/getCurrentWindow()
        // don't throw when /add or any route that uses them mounts.
        metadata: {
            currentWindow: { label: "main" },
            currentWebview: { label: "main" },
        },
        invoke: async (cmd, _args) => {
            const fn = handlers[cmd];
            if (!fn) {
                throw new Error('tauri-stub: unmocked command ' + cmd);
            }
            return fn(_args);
        },
        transformCallback: (cb) => {
            const id = Math.floor(Math.random() * 2 ** 31);
            window['_' + id] = cb;
            return id;
        },
    };
})();
`;
