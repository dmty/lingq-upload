<script lang="ts">
  import { onDestroy } from "svelte";

  import {
    commands,
    type DetectionAvailability,
    type ProjectId,
  } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import TranscribeConsentModal from "$lib/components/TranscribeConsentModal.svelte";

  let {
    projectId,
    availability,
    onAvailabilityChanged,
  }: {
    projectId: ProjectId;
    availability: DetectionAvailability | null;
    onAvailabilityChanged: (next: DetectionAvailability) => void;
  } = $props();

  let modalOpen = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  let destroyed = false;

  onDestroy(() => (destroyed = true));

  function requestDetection(event: MouseEvent) {
    trigger = event.currentTarget as HTMLButtonElement;
    if (!availability?.consent_matches) modalOpen = true;
  }

  async function acceptConsent() {
    if (!availability) return;
    const consentProjectId = projectId;
    const accepted = await commands.cmdAcceptTranscribeConsent(
      consentProjectId,
      availability.active_provider.id,
    );
    if (accepted.status === "error") {
      throw new Error(appErrorMessage(accepted.error));
    }
    if (destroyed || projectId !== consentProjectId) return;
    const refreshed = await commands.cmdDetectionAvailability(consentProjectId);
    if (refreshed.status === "error") {
      throw new Error(appErrorMessage(refreshed.error));
    }
    if (destroyed || projectId !== consentProjectId) return;
    onAvailabilityChanged(refreshed.data);
    modalOpen = false;
  }
</script>

{#if availability?.eligible}
  <section
    data-testid="detection-assist"
    class="rounded-md border border-accent-soft bg-accent-soft/30 p-4"
    aria-labelledby="detection-assist-title"
  >
    <h2 id="detection-assist-title" class="text-sm font-medium text-fg">
      Detect audio's text range
    </h2>
    <p class="mt-1 text-xs text-fg-muted">
      Optionally compare the audio boundaries with this book before choosing a
      manual response.
    </p>
    <div class="mt-3">
      {#if availability.key_present}
        <button
          bind:this={trigger}
          type="button"
          class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-canvas hover:bg-accent-hover"
          onclick={requestDetection}
        >
          Detect audio's text range
        </button>
      {:else}
        <p class="text-xs text-fg-muted">
          Add an API key for {availability.active_provider.label} to use this optional
          assist.
        </p>
        <a
          class="mt-2 inline-flex rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg no-underline hover:bg-surface-sunken hover:no-underline"
          href="/settings"
        >
          Open transcription settings
        </a>
      {/if}
    </div>
  </section>

  <TranscribeConsentModal
    provider={availability.active_provider}
    open={modalOpen}
    onAccept={acceptConsent}
    onCancel={() => (modalOpen = false)}
    returnFocusTo={trigger}
  />
{/if}
