<script lang="ts">
  type Source = "manual" | "calibre" | "libation";
  let { value = $bindable<Source>("manual") }: { value?: Source } = $props();

  const sources: { id: Source; label: string }[] = [
    { id: "manual", label: "Manual" },
    { id: "calibre", label: "Calibre" },
    { id: "libation", label: "Libation" },
  ];
</script>

<fieldset class="flex items-center gap-2">
  <legend class="sr-only">Source</legend>
  <span class="text-base text-fg-muted">Source:</span>
  <div class="segmented">
    {#each sources as source (source.id)}
      <label>
        <input
          type="radio"
          name="source"
          id={source.id}
          value={source.id}
          bind:group={value}
          class="sr-only"
        />
        {source.label}
      </label>
    {/each}
  </div>
</fieldset>
