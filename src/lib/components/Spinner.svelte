<script lang="ts">
  let {
    size = "md",
    tone = "accent",
    class: cls = "",
    ...rest
  }: {
    size?: "sm" | "md";
    tone?: "accent" | "muted";
    class?: string;
    [key: string]: unknown;
  } = $props();

  const sizes = { sm: "h-3 w-3", md: "h-4 w-4" };
  const tones = {
    accent: "border-accent border-t-transparent",
    muted: "border-fg-subtle border-t-accent",
  };
</script>

<span
  {...rest}
  class="inline-block animate-spin rounded-full border-2 {sizes[size]} {tones[tone]} {cls}"
></span>
