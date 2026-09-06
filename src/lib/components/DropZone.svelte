<script lang="ts">
  import { tick } from "svelte";
  import { basename } from "$lib/paths";
  import Button from "$lib/components/Button.svelte";

  type Variant = "text" | "audio";

  interface Props {
    variant: Variant;
    paths: string[];
    hovered: boolean;
    disabled: boolean;
    onPick: () => void;
    onRemove?: (path: string) => void;
    onClear: () => void;
    originLabel?: string;
    ref?: (el: HTMLElement | null) => void;
  }

  let {
    variant,
    paths,
    hovered,
    disabled,
    onPick,
    onRemove,
    onClear,
    originLabel,
    ref,
  }: Props = $props();

  const COPY = {
    text: {
      empty: "Drop a book here",
      hint: ".epub, .xhtml, .html, .txt",
      noun: "book file",
      clearLabel: "Clear chapter text",
    },
    audio: {
      empty: "Drop audio here",
      hint: ".m4b, .m4a, .mp3",
      noun: "audio",
      clearLabel: "Clear audio",
    },
  } as const;

  const copy = $derived(COPY[variant]);

  const mode = $derived<"empty" | "single" | "multi">(
    paths.length === 0 ? "empty" : paths.length === 1 ? "single" : "multi",
  );

  let zoneEl = $state<HTMLElement | null>(null);
  let pickButtonEl = $state<HTMLElement | null>(null);
  let rowButtonEls: Array<HTMLButtonElement | null> = $state([]);

  $effect(() => {
    ref?.(zoneEl);
    return () => ref?.(null);
  });

  async function handleRemove(path: string, index: number) {
    onRemove?.(path);
    await tick();
    // After removal the array shortened by one; the natural successor sits at
    // the same index. If we removed the tail, fall back to the new tail, then
    // to the zone's own picker when nothing remains.
    const next = rowButtonEls[index] ?? rowButtonEls[index - 1] ?? null;
    if (next) next.focus();
    else pickButtonEl?.focus();
  }

  const zoneClass = $derived(
    `flex items-center gap-3 rounded-md border-[1.5px] border-dashed px-3 py-2.5 transition-[background,border-color] duration-120 ${
      hovered
        ? "border-accent bg-accent-soft"
        : mode === "empty"
          ? "border-border-strong bg-surface"
          : "border-success bg-success-soft"
    }`,
  );
</script>

{#snippet glyph(size: number, tone: string)}
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.5"
    stroke-linecap="round"
    stroke-linejoin="round"
    class={tone}
    aria-hidden="true"
  >
    {#if variant === "text"}
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
      <polyline points="14 2 14 8 20 8" />
      <line x1="9" y1="14" x2="15" y2="14" />
      <line x1="9" y1="18" x2="13" y2="18" />
    {:else}
      <path d="M9 18V5l12-2v13" />
      <circle cx="6" cy="18" r="3" />
      <circle cx="18" cy="16" r="3" />
    {/if}
  </svg>
{/snippet}

<div class="space-y-2">
  <div bind:this={zoneEl} class={zoneClass}>
    {@render glyph(20, mode === "empty" ? "text-fg-muted" : "text-success")}

    <div class="min-w-0 flex-1">
      {#if mode === "empty"}
        <div class="text-base text-fg">{copy.empty}</div>
        <div class="text-xs text-fg-subtle">{copy.hint}</div>
      {:else if mode === "single"}
        <div class="truncate text-base text-fg" title={paths[0]}>
          {basename(paths[0])}
        </div>
      {:else}
        <div class="text-base text-fg">{paths.length} files</div>
        {#if originLabel}
          <div class="truncate text-xs text-fg-subtle">from {originLabel}</div>
        {/if}
      {/if}
    </div>

    <Button
      variant="secondary"
      aria-label={`${mode === "empty" ? "Choose" : "Change"} ${copy.noun}`}
      {disabled}
      onclick={onPick}
      bind:ref={pickButtonEl}
    >
      {mode === "empty" ? "Choose…" : "Change…"}
    </Button>

    {#if mode !== "empty"}
      <Button
        variant="secondary"
        aria-label={copy.clearLabel}
        {disabled}
        onclick={onClear}
      >
        {mode === "multi" ? "Clear all" : "Clear"}
      </Button>
    {/if}
  </div>

  {#if mode === "multi"}
    <ul class="max-h-64 overflow-y-auto rounded-sm border border-border">
      {#each paths as p, i (p)}
        <li
          class="flex items-center justify-between gap-3 border-b border-border px-2 py-1 last:border-b-0"
        >
          <span class="truncate text-base text-fg" title={p}>{basename(p)}</span
          >
          <button
            type="button"
            {disabled}
            bind:this={rowButtonEls[i]}
            title="Remove from this project"
            aria-label={`Remove ${basename(p)}`}
            onclick={() => handleRemove(p, i)}
            class="grid h-[24px] w-[24px] flex-none place-items-center rounded-sm text-fg-muted hover:bg-surface hover:text-fg disabled:opacity-50"
          >
            ×
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
