<script lang="ts">
  import type { LibraryStatus } from "$lib/ipc/bindings";

  let {
    status,
    failedReason = null,
  }: {
    status: LibraryStatus;
    failedReason?: string | null;
  } = $props();

  type Spec = {
    icon: string;
    label: string;
    classes: string;
    tooltip: string;
    spinIcon?: boolean;
  };

  const specs: Record<Exclude<LibraryStatus, "idle">, Spec> = {
    done: {
      icon: "✓",
      label: "done",
      classes: "bg-success-soft text-success",
      tooltip: "Upload complete",
    },
    running: {
      icon: "⟳",
      label: "uploading",
      classes: "bg-accent-soft text-accent",
      tooltip: "Upload in progress",
      spinIcon: true,
    },
    paused: {
      icon: "⏸",
      label: "paused",
      classes: "bg-surface-sunken text-fg-muted",
      tooltip: "Upload paused — resume to continue",
    },
    needs_match: {
      icon: "⚠",
      label: "Needs review",
      classes: "bg-warning/10 text-warning",
      tooltip: "Mapping not confirmed — review and confirm before uploading",
    },
    failed: {
      icon: "✕",
      label: "failed",
      classes: "bg-error-soft text-error",
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
    class="inline-flex items-center gap-1 rounded-sm px-2 py-0.5 text-[11px] font-medium {spec.classes}"
    title={tooltip}
  >
    <span class={spec.spinIcon ? "inline-block animate-spin" : ""}>
      {spec.icon}
    </span>
    {spec.label}
  </span>
{/if}
