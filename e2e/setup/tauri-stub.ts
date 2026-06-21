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

export function tauriStubInitScriptFor(workerIndex: number): string {
  return tauriStubInitScript.replace(
    "__WORKER_INDEX__",
    String(workerIndex),
  );
}

export const tauriStubInitScript = `
;(() => {
    if (window.__TAURI_INTERNALS__) return;

    // Mutable selection state so the picker spec can assert persistence
    // across navigation. Init scripts re-run on every page.goto so a plain
    // window-scoped object would reset. We back the skipped map with
    // sessionStorage so test-controlled navigations preserve state.
    // Namespaced by Playwright worker index so parallel workers running
    // against the same dev server don't observe each other's writes.
    const WORKER_NS = "__WORKER_INDEX__";
    const SKIPPED_KEY = "__pickerSkipped__:" + WORKER_NS;
    const MAPPING_KEY = "__mappingByProject__:" + WORKER_NS;
    function readSkipped() {
        try {
            return JSON.parse(sessionStorage.getItem(SKIPPED_KEY) || "{}");
        } catch {
            return {};
        }
    }
    function writeSkipped(m) {
        try {
            sessionStorage.setItem(SKIPPED_KEY, JSON.stringify(m));
        } catch {}
    }
    function readMappings() {
        try {
            return JSON.parse(sessionStorage.getItem(MAPPING_KEY) || "{}");
        } catch {
            return {};
        }
    }
    function writeMappings(m) {
        try {
            sessionStorage.setItem(MAPPING_KEY, JSON.stringify(m));
        } catch {}
    }
    window.__pickerState__ = {
        get skippedByProject() {
            return readSkipped();
        },
        chaptersByProject: (window.__pickerState__ && window.__pickerState__.chaptersByProject) || {},
        _writeSkipped: writeSkipped,
    };
    window.__mappingState__ = {
        get byProject() {
            return readMappings();
        },
        // Non-destructive: leaves any persisted state alone so a page.reload
        // mid-test exercises the rehydration path instead of clobbering the
        // user's in-flight edits.
        seed(key, state) {
            const m = readMappings();
            if (m[key] == null) m[key] = state;
            writeMappings(m);
        },
    };

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
        cmd_project_load: (args) => {
            const key = (args && args.key) || "stub-project";
            const skipped = readSkipped()[key] || [];
            const mapping = readMappings()[key] || null;
            return {
                schema_version: 1,
                id: {
                    content_hash: key,
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
                skipped_chapters: skipped,
                mapping: mapping,
            };
        },
        cmd_project_chapters: (args) => {
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            return window.__pickerState__.chaptersByProject[key] || [];
        },
        cmd_set_selection: (args) => {
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            const map = readSkipped();
            map[key] = (args && args.skippedIds) || [];
            writeSkipped(map);
            return null;
        },
        cmd_apply_mapping_op: (args) => {
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            const op = args && args.op;
            const expected = (args && args.expectedOpId) || 0;
            const mappings = readMappings();
            const current = mappings[key] || { pairs: [], parking_lot: [], op_id: 0 };
            if ((current.op_id || 0) + 1 !== expected) {
                throw new Error("stale op_id");
            }
            const next = JSON.parse(JSON.stringify(current));
            if (op.kind === "swap") {
                const idx = next.pairs.findIndex((p) => p.chapter_id === op.chapter_id);
                if (idx < 0) throw new Error("unknown chapter");
                const displaced = next.pairs[idx].track_id;
                const lotIdx = (next.parking_lot || []).indexOf(op.track_id);
                if (lotIdx >= 0) next.parking_lot.splice(lotIdx, 1);
                const srcIdx = next.pairs.findIndex((p) => p.track_id === op.track_id);
                if (srcIdx >= 0 && srcIdx !== idx) next.pairs[srcIdx].track_id = null;
                next.pairs[idx].track_id = op.track_id;
                next.pairs[idx].confidence = 1.0;
                next.pairs[idx].touched = true;
                if (displaced && displaced !== op.track_id) {
                    next.parking_lot = next.parking_lot || [];
                    if (!next.parking_lot.includes(displaced)) next.parking_lot.push(displaced);
                }
            } else if (op.kind === "park") {
                const idx = next.pairs.findIndex((p) => p.track_id === op.track_id);
                if (idx < 0) throw new Error("unknown track");
                next.pairs[idx].track_id = null;
                next.pairs[idx].confidence = 1.0;
                next.pairs[idx].touched = true;
                next.parking_lot = next.parking_lot || [];
                if (!next.parking_lot.includes(op.track_id)) next.parking_lot.push(op.track_id);
            } else if (op.kind === "unpark") {
                const idx = next.pairs.findIndex((p) => p.chapter_id === op.chapter_id);
                if (idx < 0) throw new Error("unknown chapter");
                const lotIdx = (next.parking_lot || []).indexOf(op.track_id);
                if (lotIdx < 0) throw new Error("unknown track");
                next.parking_lot.splice(lotIdx, 1);
                next.pairs[idx].track_id = op.track_id;
                next.pairs[idx].touched = true;
            }
            next.op_id = (current.op_id || 0) + 1;
            next.partition_locked = true;
            mappings[key] = next;
            writeMappings(mappings);
            return next;
        },
        // Mismatch inspection for the /match route. None = no decision yet,
        // route renders the empty resolver shell without any decision copy.
        // Tests can pin a fixture via window.__matcherInspection__ for the
        // single-project case, or window.__matcherInspectionByProject__ keyed
        // by content_hash to drive multi-project navigation specs.
        cmd_matcher_inspect: (args) => {
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            const byProject = window.__matcherInspectionByProject__ || {};
            if (key in byProject) return byProject[key];
            return window.__matcherInspection__ || null;
        },
        // Mirror the Rust backend: record the decision and seed an initial
        // MappingState so the next cmd_project_load returns it and the page
        // renders the mapping grid for review.
        cmd_matcher_resolve: (args) => {
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            const response = args && args.response;
            if (response === "cancel" || response === "unknown") return null;
            const chapters = window.__pickerState__.chaptersByProject[key] || [];
            const seeded = window.__matcherSeedByProject__ && window.__matcherSeedByProject__[key];
            let pairs = [];
            if (seeded && Array.isArray(seeded.pairs)) {
                pairs = seeded.pairs;
            } else if (response === "single_lesson") {
                const tid = "t0";
                pairs = chapters.map((c) => ({
                    chapter_id: c.id,
                    track_id: tid,
                    confidence: 1.0,
                    touched: false,
                    original_confidence: 1.0,
                }));
            } else if (response === "split_proportional") {
                const trackCount = (args && args.trackCount) || 1;
                const n = chapters.length;
                pairs = chapters.map((c, i) => {
                    const bucket = Math.min(
                        trackCount - 1,
                        Math.floor((i * trackCount) / Math.max(1, n)),
                    );
                    return {
                        chapter_id: c.id,
                        track_id: "t" + bucket,
                        confidence: 1.0,
                        touched: false,
                        original_confidence: 1.0,
                    };
                });
            } else {
                // pair_accept / pair_drop: positional pairing.
                const trackCount = (args && args.trackCount) || 0;
                pairs = chapters.map((c, i) => ({
                    chapter_id: c.id,
                    track_id: i < trackCount ? "t" + i : null,
                    confidence: 1.0,
                    touched: false,
                    original_confidence: 1.0,
                }));
            }
            const mappings = readMappings();
            mappings[key] = { pairs, parking_lot: [], op_id: 0 };
            writeMappings(mappings);
            return null;
        },
        // Re-split excluding a chapter. Drops the pair, updates skipped set,
        // returns the updated MappingState with partition_locked: false.
        cmd_recompute_split: (args) => {
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            const excludedId = args && args.excludedChapterId;
            const mappings = readMappings();
            const current = mappings[key] || { pairs: [], parking_lot: [], op_id: 0, partition_locked: false, buckets: [] };
            const next = JSON.parse(JSON.stringify(current));
            if (excludedId) {
                next.pairs = next.pairs.filter((p) => p.chapter_id !== excludedId);
                const skippedMap = readSkipped();
                const existing = skippedMap[key] || [];
                if (!existing.includes(excludedId)) existing.push(excludedId);
                skippedMap[key] = existing;
                writeSkipped(skippedMap);
            }
            next.partition_locked = false;
            mappings[key] = next;
            writeMappings(mappings);
            return next;
        },
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
