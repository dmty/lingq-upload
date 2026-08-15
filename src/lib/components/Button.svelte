<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    variant = "primary",
    size = "md",
    href,
    class: cls = "",
    ref = $bindable(null),
    children,
    ...rest
  }: {
    variant?: "primary" | "secondary" | "danger";
    size?: "sm" | "md" | "lg" | "xl";
    href?: string;
    class?: string;
    ref?: HTMLElement | null;
    children: Snippet;
    [key: string]: unknown;
  } = $props();

  const variants = {
    primary: "bg-accent text-accent-fg hover:bg-accent-hover disabled:bg-fg-subtle",
    secondary: "push-button text-fg",
    danger: "bg-error text-on-fill hover:bg-error/90 disabled:bg-fg-subtle",
  };
  // px, not rem: html sets font-size to --text-base (13px), so rem-based
  // control heights would render short of their intended macOS metrics.
  // md/lg deliberately match .field/.field-lg so the app has two control
  // metrics, not four.
  const sizes = {
    sm: "h-[20px] px-[8px] text-xs",
    md: "h-[24px] px-[10px] text-base",
    lg: "h-[28px] px-[14px] text-base",
    xl: "h-[36px] w-full text-md",
  };
  const classes = $derived(
    `btn inline-flex items-center justify-center gap-[8px] rounded-sm font-medium transition-colors duration-120 ease-snappy disabled:cursor-not-allowed ${variants[variant]} ${sizes[size]} ${cls}`,
  );
</script>

{#if href}
  <a {href} {...rest} bind:this={ref} class="{classes} no-underline hover:no-underline">
    {@render children()}
  </a>
{:else}
  <button type="button" {...rest} bind:this={ref} class={classes}>
    {@render children()}
  </button>
{/if}
