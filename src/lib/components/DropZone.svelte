<script lang="ts">
  import { tick } from "svelte";
  import { basename } from "$lib/paths";

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
    ref?: (el: HTMLButtonElement | null) => void;
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
      empty: "Drop chapter text or click to choose",
      hint: ".xhtml, .html, .txt",
      clearLabel: "Clear chapter text",
    },
    audio: {
      empty: "Drop audio or click to choose",
      hint: ".m4b, .m4a, .mp3",
      clearLabel: "Clear audio",
    },
  } as const;

  const copy = $derived(COPY[variant]);

  const mode = $derived<"empty" | "single" | "multi">(
    paths.length === 0 ? "empty" : paths.length === 1 ? "single" : "multi",
  );

  let buttonEl = $state<HTMLButtonElement | null>(null);
  let rowButtonEls: Array<HTMLButtonElement | null> = $state([]);

  $effect(() => {
    ref?.(buttonEl);
    return () => ref?.(null);
  });

  const summaryId = `dropzone-summary-${Math.random().toString(36).slice(2, 9)}`;

  async function handleRemove(path: string, index: number) {
    onRemove?.(path);
    await tick();
    // After removal the array shortened by one; the natural successor sits at
    // the same index. If we removed the tail, fall back to the new tail, then
    // to the outer dropzone button when nothing remains.
    const next = rowButtonEls[index] ?? rowButtonEls[index - 1] ?? null;
    if (next) next.focus();
    else buttonEl?.focus();
  }
</script>

{#if mode === "multi"}
  <div class="space-y-2">
    <button
      type="button"
      bind:this={buttonEl}
      onclick={onPick}
      {disabled}
      aria-describedby={summaryId}
      class="group flex w-full items-center gap-3 rounded-md border-[1.5px] border-dashed px-4 py-5 text-left transition-[background,border-color] duration-120 {hovered
        ? 'border-accent bg-accent-soft'
        : 'border-success bg-success-soft'}"
    >
      <svg
        width="22"
        height="22"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        class="text-success"
        aria-hidden="true"
      >
        <path d="M9 18V5l12-2v13" />
        <circle cx="6" cy="18" r="3" />
        <circle cx="18" cy="16" r="3" />
      </svg>

      <div class="flex-1">
        <div id={summaryId} class="text-sm font-medium text-fg">
          {paths.length} files
          <span class="text-fg-subtle">· click to add more</span>
        </div>
        <div class="flex items-center justify-between gap-3">
          <div class="text-xs text-fg-subtle">
            {#if originLabel}
              from {originLabel}
            {/if}
          </div>
          <span
            role="button"
            tabindex="0"
            aria-disabled={disabled}
            onclick={(e) => {
              e.stopPropagation();
              if (disabled) return;
              onClear();
            }}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.stopPropagation();
                e.preventDefault();
                if (disabled) return;
                onClear();
              }
            }}
            class="rounded-sm px-2 py-1 text-xs text-fg-muted hover:bg-surface hover:text-fg {disabled
              ? 'pointer-events-none opacity-50'
              : ''}"
          >
            Clear all
          </span>
        </div>
      </div>
    </button>

    <ul
      class="max-h-64 overflow-y-auto rounded-md border border-border-strong/40"
    >
      {#each paths as p, i (p)}
        <li class="flex items-center justify-between gap-3 px-3 py-1.5">
          <span class="truncate text-sm text-fg" title={p}>{basename(p)}</span>
          <button
            type="button"
            {disabled}
            bind:this={rowButtonEls[i]}
            aria-label={`Remove ${basename(p)}`}
            onclick={() => handleRemove(p, i)}
            class="rounded-sm px-2 py-1 text-xs text-fg-muted hover:bg-surface hover:text-fg disabled:opacity-50"
          >
            ×
          </button>
        </li>
      {/each}
    </ul>
  </div>
{:else}
  <button
    type="button"
    bind:this={buttonEl}
    onclick={onPick}
    {disabled}
    class="group flex items-center gap-3 rounded-md border-[1.5px] border-dashed px-4 py-5 text-left transition-[background,border-color] duration-120 {hovered
      ? 'border-accent bg-accent-soft'
      : mode === 'single'
        ? 'border-success bg-success-soft'
        : 'border-border-strong bg-surface hover:border-accent hover:bg-accent-soft'}"
  >
    {#if variant === "text"}
      <svg
        width="22"
        height="22"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        class={mode === "single" ? "text-success" : "text-fg-muted"}
        aria-hidden="true"
      >
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <polyline points="14 2 14 8 20 8" />
        <line x1="9" y1="14" x2="15" y2="14" />
        <line x1="9" y1="18" x2="13" y2="18" />
      </svg>
    {:else}
      <svg
        width="22"
        height="22"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        class={mode === "single" ? "text-success" : "text-fg-muted"}
        aria-hidden="true"
      >
        <path d="M9 18V5l12-2v13" />
        <circle cx="6" cy="18" r="3" />
        <circle cx="18" cy="16" r="3" />
      </svg>
    {/if}

    <div class="flex-1">
      {#if mode === "single"}
        <div class="text-sm font-medium text-fg">{basename(paths[0])}</div>
        <div class="text-xs text-fg-subtle">
          Click to choose a different file
        </div>
      {:else}
        <div class="text-sm font-medium text-fg">{copy.empty}</div>
        <div class="text-xs text-fg-subtle">{copy.hint}</div>
      {/if}
    </div>

    {#if mode === "single"}
      <span
        role="button"
        tabindex="0"
        aria-label={copy.clearLabel}
        onclick={(e) => {
          e.stopPropagation();
          onClear();
        }}
        onkeydown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.stopPropagation();
            e.preventDefault();
            onClear();
          }
        }}
        class="rounded-sm px-2 py-1 text-xs text-fg-muted hover:bg-surface hover:text-fg"
      >
        ×
      </span>
    {/if}
  </button>
{/if}
