<script lang="ts">
  type Source = "manual" | "calibre" | "libation";
  let { value = $bindable<Source>("manual") }: { value?: Source } = $props();
</script>

<fieldset class="space-y-2">
  <legend class="text-xs font-medium uppercase tracking-wide text-fg-muted">
    Source
  </legend>
  <div class="flex gap-2">
    {#each ["manual", "calibre", "libation"] as id (id)}
      <label
        class="flex-1 min-h-[24px] cursor-pointer rounded-sm border border-border bg-surface px-3 py-2 text-sm transition-colors duration-120 hover:bg-surface-sunken has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-fg has-[:focus-visible]:outline-offset-1 {value ===
        id
          ? 'border-accent ring-1 ring-accent'
          : ''}"
      >
        <input
          type="radio"
          name="source"
          {id}
          value={id}
          bind:group={value}
          class="sr-only"
        />
        <span class="capitalize">{id}</span>
      </label>
    {/each}
  </div>
</fieldset>
