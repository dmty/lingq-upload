<script lang="ts">
  import type { AbsorbPolicy } from "$lib/ipc/bindings";

  export let absorbPolicy: AbsorbPolicy = "forward";

  // TODO: wire onchange to a Tauri command once the persistence path lands.
  // For now this is a pure-presentation widget rendered by the project
  // settings sheet; the parent component reads `absorbPolicy` back out of
  // bound state when the user saves.
  const policies: Array<{ value: AbsorbPolicy; label: string; hint: string }> = [
    {
      value: "forward",
      label: "Forward",
      hint: "Silence joins the next chapter (default)."
    },
    {
      value: "backward",
      label: "Backward",
      hint: "Silence joins the previous chapter."
    },
    {
      value: "drop",
      label: "Drop",
      hint: "Silence is excluded from both sides."
    }
  ];
</script>

<fieldset class="absorb-policy">
  <legend>Chapter-divider silence</legend>
  {#each policies as p (p.value)}
    <label>
      <input
        type="radio"
        name="absorb-policy"
        value={p.value}
        bind:group={absorbPolicy}
      />
      <span class="label">{p.label}</span>
      <span class="hint">{p.hint}</span>
    </label>
  {/each}
</fieldset>

<style>
  .absorb-policy {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    border: 1px solid var(--border, #ccc);
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
  }
  legend {
    font-weight: 600;
    padding: 0 0.25rem;
  }
  label {
    display: grid;
    grid-template-columns: auto 6rem 1fr;
    align-items: baseline;
    gap: 0.5rem;
    cursor: pointer;
  }
  .label {
    font-weight: 500;
  }
  .hint {
    color: var(--muted, #666);
    font-size: 0.9em;
  }
</style>
