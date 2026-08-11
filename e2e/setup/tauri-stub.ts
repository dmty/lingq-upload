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
  return tauriStubInitScript.replace("__WORKER_INDEX__", String(workerIndex));
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
    const DECISION_KEY = "__matcherDecisionByProject__:" + WORKER_NS;
    const TRANSCRIPTION_PREFERENCES_KEY = "__transcriptionPreferences__:" + WORKER_NS;
    const TRANSCRIPTION_KEYS_KEY = "__transcriptionKeyPresence__:" + WORKER_NS;
    const TRANSCRIPTION_CONSENTS_KEY = "__transcriptionConsents__:" + WORKER_NS;
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
    // Matcher decisions are sessionStorage-backed so a confirm/reset round trip
    // survives the navigations and reloads the specs drive. Seeded from
    // window.__matcherDecisionByProject__ until the first write.
    function readDecisions() {
        try {
            const raw = sessionStorage.getItem(DECISION_KEY);
            if (raw) return JSON.parse(raw);
        } catch {}
        return window.__matcherDecisionByProject__ || {};
    }
    function writeDecisions(m) {
        try {
            sessionStorage.setItem(DECISION_KEY, JSON.stringify(m));
        } catch {}
    }
    function readReceipts(key) {
        return (window.__receiptsByProject__ && window.__receiptsByProject__[key]) || [];
    }
    function readTranscriptionPreferences() {
        try {
            return JSON.parse(sessionStorage.getItem(TRANSCRIPTION_PREFERENCES_KEY)) || {
                provider_id: "groq",
                auto_detect_start: false,
            };
        } catch {
            return { provider_id: "groq", auto_detect_start: false };
        }
    }
    function writeTranscriptionPreferences(preferences) {
        try {
            sessionStorage.setItem(
                TRANSCRIPTION_PREFERENCES_KEY,
                JSON.stringify(preferences),
            );
        } catch {}
    }
    function readTranscriptionKeys() {
        try {
            return JSON.parse(sessionStorage.getItem(TRANSCRIPTION_KEYS_KEY)) ||
                window.__transcriptionKeys__ || {};
        } catch {
            return window.__transcriptionKeys__ || {};
        }
    }
    function writeTranscriptionKeys(keys) {
        try {
            sessionStorage.setItem(TRANSCRIPTION_KEYS_KEY, JSON.stringify(keys));
        } catch {}
    }
    function readTranscriptionConsents() {
        try {
            return JSON.parse(sessionStorage.getItem(TRANSCRIPTION_CONSENTS_KEY)) ||
                window.__transcriptionConsents__ || {};
        } catch {
            return window.__transcriptionConsents__ || {};
        }
    }
    function writeTranscriptionConsents(consents) {
        try {
            sessionStorage.setItem(
                TRANSCRIPTION_CONSENTS_KEY,
                JSON.stringify(consents),
            );
        } catch {}
    }
    function transcriptionProviders() {
        const keys = readTranscriptionKeys();
        return [
            {
                id: "groq",
                label: "Groq",
                model: "whisper-large-v3-turbo",
                pricing_hint: {
                    summary: "Free-tier eligible; limits depend on your account/tier; current paid reference $0.04/hour",
                    estimated_usd_per_minute: 0.0006666666666666666,
                    free_tier_eligible: true,
                    docs_url: "https://console.groq.com/docs/speech-to-text",
                },
                data_policy_url: "https://console.groq.com/docs/your-data",
                key_present: keys.groq === true,
            },
            {
                id: "open_ai",
                label: "OpenAI",
                model: "whisper-1",
                pricing_hint: {
                    summary: "No free tier; current reference $0.006/min",
                    estimated_usd_per_minute: 0.006,
                    free_tier_eligible: false,
                    docs_url: "https://developers.openai.com/api/docs/models/whisper-1",
                },
                data_policy_url: "https://platform.openai.com/docs/models/default-usage-policies-by-endpoint",
                key_present: keys.open_ai === true,
            },
        ];
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

    window.__invokeLog__ = [];
    window.__eventHandlers__ = window.__eventHandlers__ || {};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener(event, id) {
            const ids = window.__eventHandlers__[event] || [];
            window.__eventHandlers__[event] = ids.filter((candidate) => candidate !== id);
            delete window["_" + id];
        },
    };
    // Test seam: fire a Tauri event into every listener the app registered.
    window.__emitEvent__ = (event, payload) => {
        const ids = window.__eventHandlers__[event] || [];
        for (const id of ids) {
            const cb = window["_" + id];
            if (cb) cb({ event, id, payload });
        }
    };

    const handlers = {
        // Library list returns entries from window.__libraryEntries__ if set,
        // else empty index so the empty-state CTA renders. Specs can hold the
        // response open via window.__libraryGate__ to exercise the moment
        // between navigation and the index resolving, or fail it outright via
        // window.__libraryError__.
        cmd_library_list: async () => {
            if (window.__libraryGate__) await window.__libraryGate__;
            if (window.__libraryError__) throw window.__libraryError__;
            return {
                schema_version: 1,
                generated_at: new Date().toISOString(),
                entries: window.__libraryEntries__ || [],
            };
        },
        // Account/profile probe — empty placeholder keeps the layout calm.
        cmd_account_profile: () => ({ username: null, known_words: {} }),
        // Load LingQ key, defaulting to null if not set in the test env.
        cmd_load_lingq_key: () => window.__lingqKey__ ?? null,
        // Project load for the run screen. Returns a minimal project with
        // no receipts so the chapter list renders empty and the start
        // button shows "Start" (not "Resume" — which would tax the
        // forbidden-word filter on /run).
        cmd_project_load: (args) => {
            const key = (args && args.key) || "stub-project";
            if (window.__projectByKey__ && key in window.__projectByKey__) {
                return window.__projectByKey__[key];
            }
            const skipped = readSkipped()[key] || [];
            const mapping = readMappings()[key] || null;
            const meta = (window.__projectMeta__ && window.__projectMeta__[key]) || {};
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
                    collection_title: meta.title || "Stub Project",
                    level: 1,
                    tags: [],
                },
                receipts: readReceipts(key),
                queue_cursor: 0,
                completed_lesson_ids: [],
                matcher_decision: readDecisions()[key] ?? null,
                cover_path: meta.cover_path ?? null,
                authors: meta.authors || [],
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
        // Mirrors the real command: a project with no audio source returns
        // an empty plan rather than an error. Specs that need a seeded
        // queue set window.__planByKey__[key].
        cmd_project_plan_preview: (args) => {
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            return (window.__planByKey__ && window.__planByKey__[key]) || [];
        },
        cmd_chapter_text: (args) => {
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            const chapterId = args && args.chapterId;
            const chapters = window.__pickerState__.chaptersByProject[key] ?? [];
            const found = chapters.find((c) => c.id === chapterId);
            return found?.body ?? "";
        },
        cmd_set_selection: (args) => {
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            const map = readSkipped();
            map[key] = (args && args.skippedIds) || [];
            writeSkipped(map);
            return null;
        },
        cmd_set_cover: () => null,
        "plugin:dialog|open": () => window.__dialogPickPath__ ?? null,
        // Records the URL passed to openUrl() so specs can assert on it
        // directly, rather than only checking that the trigger element exists.
        "plugin:opener|open_url": (args) => {
            window.__openedUrl__ = args && args.url;
            return null;
        },
        cmd_apply_mapping_op: (args) => {
            if (window.__failNextMappingOp__) {
                window.__failNextMappingOp__ = false;
                throw { kind: "MappingStaleOp", message: { server: 9, expected: 1 } };
            }
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
            } else if (op.kind === "reassign") {
                const idx = next.pairs.findIndex((p) => p.chapter_id === op.chapter_id);
                if (idx < 0) throw new Error("unknown chapter");
                next.pairs[idx].track_id = op.track_id;
                next.pairs[idx].confidence = 1.0;
                next.pairs[idx].touched = true;
            }
            next.op_id = (current.op_id || 0) + 1;
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
        cmd_confirm_mapping: (args) => {
            window.__confirmMappingCalls__ = window.__confirmMappingCalls__ || [];
            window.__confirmMappingCalls__.push(args);
            return null;
        },
        cmd_start_project_job: () => "job-1",
        cmd_project_cancel: () => 1,
        cmd_cancel_job: (args) => {
            window.__cancelJobCalls__ = window.__cancelJobCalls__ || [];
            window.__cancelJobCalls__.push(args);
            return null;
        },
        cmd_list_languages: () => window.__languages__ || [],
        cmd_list_collections: () => window.__collections__ || [],
        // Course detail for the /course route. Specs pin the payload via
        // window.__courseView__ and force failures via window.__courseError__.
        //
        // The call counter lives in sessionStorage, not on window: init scripts
        // re-run on every page.goto, so a window counter would reset to 0 and a
        // "did not refetch" assertion would pass even when suppression is broken.
        cmd_lingq_course: async () => {
            const COUNT_KEY = "__courseFetchCount__:" + WORKER_NS;
            const seen = Number(sessionStorage.getItem(COUNT_KEY) || "0");
            sessionStorage.setItem(COUNT_KEY, String(seen + 1));
            if (window.__courseGate__) await window.__courseGate__;
            if (window.__courseError__) throw window.__courseError__;
            return (
                window.__courseView__ ?? {
                    collection: {
                        id: 0,
                        title: "",
                        description: null,
                        level: null,
                        duration: null,
                        lessons_count: null,
                        new_words_count: null,
                        image_url: null,
                        status: null,
                        roses_count: null,
                        views_count: null,
                    },
                    lessons: [],
                }
            );
        },
        manual_source_from_files: () => ({ stub: true }),
        upload_one_shot: async () => {
            if (window.__uploadOneShotGate__) await window.__uploadOneShotGate__;
            if (window.__uploadOneShotError__) throw window.__uploadOneShotError__;
            return window.__uploadOneShotResult__ ?? { lesson_id: 101, collection_id: 7 };
        },
        cmd_save_lingq_key: (args) => {
            window.__lingqKey__ = (args && args.key) || null;
            return null;
        },
        cmd_clear_lingq_key: () => {
            window.__lingqKey__ = null;
            return null;
        },
        cmd_list_transcribe_providers: () => transcriptionProviders(),
        cmd_save_transcribe_key: (args) => {
            const keys = readTranscriptionKeys();
            keys[args && args.provider] = Boolean(args && args.key);
            writeTranscriptionKeys(keys);
            return null;
        },
        cmd_transcribe_key_present: (args) =>
            readTranscriptionKeys()[args && args.provider] === true,
        cmd_clear_transcribe_key: (args) => {
            const keys = readTranscriptionKeys();
            keys[args && args.provider] = false;
            writeTranscriptionKeys(keys);
            return null;
        },
        cmd_get_transcription_preferences: () => readTranscriptionPreferences(),
        cmd_set_transcription_preferences: (args) => {
            if (window.__failNextTranscriptionPreferences__) {
                window.__failNextTranscriptionPreferences__ = false;
                throw { kind: "Other", message: "Could not save transcription preferences." };
            }
            writeTranscriptionPreferences(args && args.preferences);
            return null;
        },
        cmd_detection_availability: (args) => {
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            const byProject = window.__detectionAvailabilityByProject__ || {};
            const base = byProject[key] || window.__detectionAvailability__ || {
                eligible: false,
                condition: null,
                chapter_count: 0,
                track_count: 0,
                existing_evidence: null,
            };
            const providerId = readTranscriptionPreferences().provider_id;
            const configuredProvider = transcriptionProviders().find(
                (provider) => provider.id === providerId,
            );
            const keyPresent = base.key_present ?? configuredProvider.key_present;
            const activeProvider = {
                ...configuredProvider,
                key_present: keyPresent,
            };
            const consentMatches =
                readTranscriptionConsents()[key] === activeProvider.id;
            return {
                ...base,
                active_provider: activeProvider,
                key_present: keyPresent,
                consent_matches: consentMatches,
                can_start:
                    base.eligible &&
                    base.existing_evidence == null &&
                    keyPresent &&
                    consentMatches,
            };
        },
        cmd_accept_transcribe_consent: async (args) => {
            window.__transcribeConsentCalls__ = window.__transcribeConsentCalls__ || [];
            window.__transcribeConsentCalls__.push(args);
            if (window.__failNextTranscribeConsent__) {
                window.__failNextTranscribeConsent__ = false;
                throw { kind: "Other", message: "Could not save transcription consent." };
            }
            if (window.__transcribeConsentGate__) {
                await window.__transcribeConsentGate__;
            }
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            const consents = readTranscriptionConsents();
            consents[key] = args && args.providerId;
            writeTranscriptionConsents(consents);
            return null;
        },
        cmd_detect_start_offset: async (args) => {
            window.__detectionStartCalls__ =
                (window.__detectionStartCalls__ || 0) + 1;
            window.__detectionStartArgs__ = window.__detectionStartArgs__ || [];
            window.__detectionStartArgs__.push(args);
            window.__detectionListenerCountAtStart__ =
                (window.__eventHandlers__.job || []).length;
            if (window.__detectionGate__) await window.__detectionGate__;
            if (window.__detectionCommandError__) throw window.__detectionCommandError__;
            return window.__detectionResult__ || {
                kind: "no_transcript",
                reason: "content_poor",
            };
        },
        cmd_confirm_detected_range: async (args) => {
            window.__confirmDetectedRangeCalls__ =
                window.__confirmDetectedRangeCalls__ || [];
            window.__confirmDetectedRangeCalls__.push(args);
            if (window.__confirmDetectedRangeGate__) {
                await window.__confirmDetectedRangeGate__;
            }
            if (window.__confirmDetectedRangeError__) {
                throw window.__confirmDetectedRangeError__;
            }
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            const range = args && args.selectedRange;
            const chapters = window.__pickerState__.chaptersByProject[key] || [];
            const start = chapters.findIndex((chapter) =>
                chapter.id === range.start_chapter_id
            );
            const end = chapters.findIndex((chapter) =>
                chapter.id === range.end_chapter_id
            );
            const selected = chapters.slice(start, end + 1);
            const skipped = readSkipped();
            skipped[key] = chapters
                .filter((chapter) => !selected.some((item) => item.id === chapter.id))
                .map((chapter) => chapter.id);
            writeSkipped(skipped);
            const mappings = readMappings();
            mappings[key] = {
                pairs: selected.map((chapter, index) => ({
                    chapter_id: chapter.id,
                    track_id: "t" + index,
                    confidence: 1,
                    original_confidence: 1,
                    touched: false,
                })),
                parking_lot: [],
                op_id: 0,
            };
            writeMappings(mappings);
            const decisions = readDecisions();
            decisions[key] = {
                condition: "many_to_few",
                response: "split_proportional",
                chapter_count: chapters.length,
                track_count: selected.length,
                user_overrode: false,
                decided_at: new Date().toISOString(),
                detection: { ...(args && args.evidence), range: range },
            };
            writeDecisions(decisions);
            return null;
        },
        // Mirrors reset_detection_impl: refuses once uploads exist, otherwise
        // drops the decision and mapping so the resolver renders again.
        cmd_reset_detection: async (args) => {
            window.__resetDetectionCalls__ = window.__resetDetectionCalls__ || [];
            window.__resetDetectionCalls__.push(args);
            if (window.__resetDetectionGate__) await window.__resetDetectionGate__;
            if (window.__resetDetectionError__) throw window.__resetDetectionError__;
            const pid = args && args.projectId;
            const key = (pid && pid.content_hash) || "stub-project";
            if (readReceipts(key).length > 0) {
                throw {
                    kind: "Unsupported",
                    message: "cannot reset detection after uploads have begun",
                };
            }
            const decisions = readDecisions();
            delete decisions[key];
            writeDecisions(decisions);
            const mappings = readMappings();
            delete mappings[key];
            writeMappings(mappings);
            return null;
        },
        cmd_seed_mapping: () => null,
        // Dev backend info — defaults to file backend, debug mode off.
        cmd_get_dev_backend: () => ({
            is_debug: false,
            current: "file",
            env_override: false,
        }),
        cmd_set_dev_backend: () => null,
        // Trash list for the /settings route. Empty list keeps the panel quiet.
        cmd_list_trash: () => [],
        // Listeners are recorded; specs drive them via window.__emitEvent__.
        "plugin:event|listen": (args) => {
            const id = (args && args.handler) || 0;
            const ev = args && args.event;
            (window.__eventHandlers__[ev] = window.__eventHandlers__[ev] || []).push(id);
            return id;
        },
        "plugin:event|unlisten": (args) => {
            const ids = window.__eventHandlers__[args && args.event] || [];
            window.__eventHandlers__[args && args.event] = ids.filter(
                (candidate) => candidate !== (args && args.eventId),
            );
            return null;
        },
    };

    window.__TAURI_INTERNALS__ = {
        // Webview/window metadata so getCurrentWebview()/getCurrentWindow()
        // don't throw when /add or any route that uses them mounts.
        metadata: {
            currentWindow: { label: "main" },
            currentWebview: { label: "main" },
        },
        invoke: async (cmd, _args) => {
            window.__invokeLog__.push(cmd);
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
        convertFileSrc: (p) => "asset://localhost/" + encodeURIComponent(p),
    };
})();
`;
