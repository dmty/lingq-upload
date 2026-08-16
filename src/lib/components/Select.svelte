<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    value = $bindable(),
    size = "lg",
    class: cls = "",
    children,
    ...rest
  }: {
    value?: unknown;
    size?: "md" | "lg";
    class?: string;
    children: Snippet;
    [key: string]: unknown;
  } = $props();
</script>

<span class="select-wrap {cls}">
  <select bind:value {...rest} class="field {size === 'lg' ? 'field-lg' : ''}">
    {@render children()}
  </select>
  <span class="select-badge" aria-hidden="true">
    <svg
      width="8"
      height="12"
      viewBox="0 0 8 12"
      fill="none"
      stroke="currentColor"
      stroke-width="1.4"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M1.8 4.8 4 2.4l2.2 2.4" />
      <path d="M1.8 7.2 4 9.6l2.2-2.4" />
    </svg>
  </span>
</span>
