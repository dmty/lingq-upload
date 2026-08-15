<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    variant = "primary",
    size = "md",
    href,
    class: cls = "",
    children,
    ...rest
  }: {
    variant?: "primary" | "secondary" | "danger";
    size?: "sm" | "md" | "lg" | "xl";
    href?: string;
    class?: string;
    children: Snippet;
    [key: string]: unknown;
  } = $props();

  const variants = {
    primary: "bg-accent text-accent-fg hover:bg-accent-hover disabled:bg-fg-subtle",
    secondary:
      "border border-border bg-surface text-fg hover:bg-surface-sunken disabled:opacity-50",
    danger: "bg-error text-white hover:bg-error/90 disabled:bg-fg-subtle",
  };
  const sizes = {
    sm: "px-3 py-1 text-xs",
    md: "px-3 py-1.5 text-sm",
    lg: "px-4 py-2 text-sm",
    xl: "h-12 w-full text-base",
  };
  const classes = $derived(
    `inline-flex items-center justify-center gap-2 rounded-sm font-medium transition-colors duration-120 ease-snappy disabled:cursor-not-allowed ${variants[variant]} ${sizes[size]} ${cls}`,
  );
</script>

{#if href}
  <a {href} {...rest} class="{classes} no-underline hover:no-underline">
    {@render children()}
  </a>
{:else}
  <button type="button" {...rest} class={classes}>
    {@render children()}
  </button>
{/if}
