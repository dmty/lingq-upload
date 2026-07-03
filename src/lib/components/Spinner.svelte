<script lang="ts">
  let { class: cls = "", ...rest }: { class?: string; [key: string]: unknown } = $props();
</script>

<span
  {...rest}
  class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-accent border-t-transparent {cls}"
></span>
