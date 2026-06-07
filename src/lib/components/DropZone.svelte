<script lang="ts">
  import { basename } from "$lib/paths";

  type Variant = "text" | "audio";

  interface Props {
    variant: Variant;
    path: string;
    hovered: boolean;
    disabled: boolean;
    onPick: () => void;
    onClear: () => void;
    ref?: (el: HTMLButtonElement | null) => void;
  }

  let { variant, path, hovered, disabled, onPick, onClear, ref }: Props =
    $props();

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

  let buttonEl = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    ref?.(buttonEl);
    return () => ref?.(null);
  });
</script>

<button
  type="button"
  bind:this={buttonEl}
  onclick={onPick}
  {disabled}
  class="group flex items-center gap-3 rounded-md border-[1.5px] border-dashed px-4 py-5 text-left transition-[background,border-color] duration-120 {hovered
    ? 'border-accent bg-accent-soft'
    : path
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
      class={path ? "text-success" : "text-fg-muted"}
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
      class={path ? "text-success" : "text-fg-muted"}
      aria-hidden="true"
    >
      <path d="M9 18V5l12-2v13" />
      <circle cx="6" cy="18" r="3" />
      <circle cx="18" cy="16" r="3" />
    </svg>
  {/if}

  <div class="flex-1">
    {#if path}
      <div class="text-sm font-medium text-fg">{basename(path)}</div>
      <div class="text-xs text-fg-subtle">Click to choose a different file</div>
    {:else}
      <div class="text-sm font-medium text-fg">{copy.empty}</div>
      <div class="text-xs text-fg-subtle">{copy.hint}</div>
    {/if}
  </div>

  {#if path}
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
