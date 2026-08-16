<script lang="ts">
  import { tick } from "svelte";

  import type { ProviderInfo } from "$lib/ipc/bindings";
  import Button from "$lib/components/Button.svelte";

  let {
    provider,
    open,
    onAccept,
    onCancel,
    returnFocusTo,
  }: {
    provider: ProviderInfo;
    open: boolean;
    onAccept: () => Promise<void>;
    onCancel: () => void;
    returnFocusTo: HTMLElement | null;
  } = $props();

  let dialog = $state<HTMLDialogElement | null>(null);
  let cancelButton = $state<HTMLElement | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) {
      error = null;
      dialog.showModal();
      void tick().then(() => cancelButton?.focus());
    } else if (!open && dialog.open) {
      dialog.close();
    }
  });

  function cancel() {
    if (!busy) onCancel();
  }

  async function accept() {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await onAccept();
    } catch (cause) {
      error =
        cause instanceof Error
          ? cause.message
          : "Could not save transcription consent.";
    } finally {
      busy = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      cancel();
      return;
    }
    if (event.key !== "Tab" || !dialog) return;
    const focusable = Array.from(
      dialog.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    );
    const first = focusable[0];
    const last = focusable.at(-1);
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="transcribe-consent-title"
  aria-describedby="transcribe-consent-description"
  aria-modal="true"
  onkeydown={handleKeydown}
  oncancel={(event) => {
    event.preventDefault();
    cancel();
  }}
  onclose={() => returnFocusTo?.focus()}
>
  <div class="modal-card sheet-card">
    <h2
      id="transcribe-consent-title"
      class="title text-lg font-semibold text-fg"
    >
      Allow {provider.label} transcription?
    </h2>

    <Button
      variant="secondary"
      class="cancel"
      bind:ref={cancelButton}
      disabled={busy}
      onclick={cancel}
    >
      Cancel
    </Button>

    <div
      id="transcribe-consent-description"
      class="copy space-y-3 text-sm text-fg-muted"
    >
      <p>Stage A checks embedded audio titles locally and is free.</p>
      <p>
        If that check is inconclusive, {provider.label} receives two 30-second clips
        under normal conditions, one from each end. There is at most one retry per
        side: maximum four calls / two minutes of audio.
      </p>
      <p>
        We may send optional book title and author prompt metadata to improve
        matching.
      </p>
      <p>
        Current qualified estimate: {provider.pricing_hint.summary}. Pricing can
        change; check the provider documentation.
      </p>
      <p>Your {provider.label} API key stays in your OS keychain.</p>
      <p>
        <a
          class="text-accent hover:underline"
          href={provider.data_policy_url}
          target="_blank"
          rel="noopener noreferrer"
          aria-label={`${provider.label} data policy`}
        >
          Review {provider.label}'s data policy
        </a>
      </p>
    </div>

    {#if error}
      <p class="error text-sm text-error" role="alert" aria-live="assertive">
        {error}
      </p>
    {/if}

    <Button class="accept" disabled={busy} onclick={accept}>
      {busy ? "Saving…" : "Accept and continue"}
    </Button>
  </div>
</dialog>

<style>
  /* px, not rem: exact pixel sizing, literal so it can't drift with the
     type scale — see src/app.css. */
  dialog {
    width: min(576px, calc(100vw - 32px));
  }

  .modal-card {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 16px 8px;
  }

  .title,
  .copy,
  .error {
    grid-column: 1 / -1;
  }

  .title {
    grid-row: 1;
  }

  .copy {
    grid-row: 2;
  }

  .error {
    grid-row: 3;
  }

  /* :global — cancel/accept now land on the element Button.svelte renders,
     which doesn't carry this file's scope hash. Scoped to .modal-card so the
     rule stays local instead of matching any .cancel/.accept in the app. */
  .modal-card :global(.cancel) {
    grid-column: 2;
  }

  .modal-card :global(.accept) {
    grid-column: 3;
  }

  .modal-card :global(.cancel),
  .modal-card :global(.accept) {
    grid-row: 4;
  }
</style>
