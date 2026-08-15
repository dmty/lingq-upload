<script lang="ts">
  import Spinner from "$lib/components/Spinner.svelte";
  import type { LibraryStatus } from "$lib/ipc/bindings";
  import { statusLabel } from "$lib/status-labels";

  let {
    status,
    failedReason = null,
  }: {
    status: LibraryStatus;
    failedReason?: string | null;
  } = $props();

  type Spec = {
    label: string;
    classes: string;
    tooltip: string;
    spinIcon?: boolean;
  };

  const specs: Record<Exclude<LibraryStatus, "idle">, Spec> = {
    done: {
      label: statusLabel("done"),
      classes: "bg-success",
      tooltip: "Upload complete",
    },
    running: {
      label: statusLabel("running"),
      classes: "bg-accent",
      tooltip: "Upload in progress",
      spinIcon: true,
    },
    paused: {
      label: statusLabel("paused"),
      classes: "bg-fg-subtle",
      tooltip: "Upload paused — resume to continue",
    },
    needs_match: {
      label: statusLabel("needs_match"),
      classes: "bg-warning",
      tooltip: "Mapping not confirmed — review and confirm before uploading",
    },
    failed: {
      label: statusLabel("failed"),
      classes: "bg-error",
      tooltip: "Upload failed",
    },
  };

  const spec = $derived(status === "idle" ? null : specs[status]);
  const tooltip = $derived(
    spec
      ? status === "failed" && failedReason
        ? `${spec.tooltip}: ${failedReason}`
        : spec.tooltip
      : "",
  );
</script>

{#if spec}
  <span
    class="inline-flex items-center gap-[6px] text-xs font-medium text-fg-muted"
    title={tooltip}
  >
    {#if spec.spinIcon}
      <Spinner size="sm" aria-hidden="true" />
    {:else}
      <span
        class="status-dot h-[8px] w-[8px] flex-none rounded-full {spec.classes}"
        aria-hidden="true"
      ></span>
    {/if}
    {spec.label}
  </span>
{/if}
