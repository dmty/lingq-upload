<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { commands } from "$lib/ipc/bindings";

  let pingMsg = $state<string>("…");
  let pingErr = $state<string | null>(null);
  let log = $state<string[]>([]);
  let busy = $state(false);

  let unlisten: UnlistenFn | undefined;

  onMount(() => {
    (async () => {
      try {
        const res = await commands.ping();
        // tauri-specta rc.21 wraps fallible commands in { status, data } | { status, error }.
        if (typeof res === "string") {
          pingMsg = res;
        } else if (res && typeof res === "object" && "status" in res) {
          const r = res as { status: string; data?: unknown; error?: unknown };
          if (r.status === "ok") pingMsg = String(r.data);
          else {
            pingErr = JSON.stringify(r.error);
            pingMsg = "error";
          }
        } else {
          pingMsg = String(res);
        }
      } catch (e) {
        pingErr = String(e);
        pingMsg = "error";
      }

      unlisten = await listen("job", (event) => {
        log = [...log, JSON.stringify(event.payload)];
      });
    })();

    return () => {
      unlisten?.();
    };
  });

  async function startDemo() {
    busy = true;
    try {
      await commands.startDemoJob();
    } catch (e) {
      log = [...log, `error: ${String(e)}`];
    } finally {
      busy = false;
    }
  }
</script>

<section>
  <h1>lingq-upload</h1>

  <p>
    Backend says: <strong>{pingMsg}</strong>
    {#if pingErr}<span class="err"> ({pingErr})</span>{/if}
  </p>

  <button onclick={startDemo} disabled={busy}>
    {busy ? "Running…" : "Start demo job"}
  </button>

  <h2>Event log</h2>
  <pre class="log">{log.length === 0 ? "(no events yet)" : log.join("\n")}</pre>
</section>

<style>
  section {
    max-width: 720px;
    margin: 0 auto;
  }

  .log {
    background: #111;
    color: #0f0;
    padding: 0.75rem;
    border-radius: 4px;
    font-size: 0.8rem;
    max-height: 320px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .err {
    color: #c00;
    font-size: 0.85rem;
  }

  button {
    padding: 0.5rem 1rem;
    border-radius: 6px;
    border: 1px solid #535bf2;
    background: #535bf2;
    color: white;
    font-weight: 500;
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
