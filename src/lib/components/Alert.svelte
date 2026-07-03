<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    variant = "error",
    body = false,
    class: cls = "",
    children,
    ...rest
  }: {
    variant?: "error" | "warning" | "success";
    body?: boolean;
    class?: string;
    children: Snippet;
    [key: string]: unknown;
  } = $props();

  const styles = {
    error: "border-error bg-error-soft",
    warning: "border-warning bg-warning-soft",
    success: "border-success bg-success-soft",
  };
  const text = $derived(body || variant !== "error" ? "text-fg" : "text-error");
</script>

<div
  {...rest}
  role={variant === "error" ? "alert" : "status"}
  class="rounded-sm border-l-[3px] p-3 text-sm {styles[variant]} {text} {cls}"
>
  {@render children()}
</div>
