<script lang="ts">
  import { commands, type AbsorbPolicy, type ProjectId } from "$lib/ipc/bindings";

  let {
    projectId,
    absorbPolicy = $bindable("forward"),
  }: {
    projectId: ProjectId;
    absorbPolicy?: AbsorbPolicy;
  } = $props();

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

  // Namespace the radio group so two ProjectSettings instances can't collide.
  const groupName = $derived(`absorb-policy-${projectId.content_hash}`);

  let lastConfirmed: AbsorbPolicy = $state(absorbPolicy);
  let timer: ReturnType<typeof setTimeout> | null = null;

  function onSelect(next: AbsorbPolicy) {
    if (next === absorbPolicy) return;
    const prev = lastConfirmed;
    absorbPolicy = next;
    if (timer != null) clearTimeout(timer);
    timer = setTimeout(async () => {
      const res = await commands.cmdSetAbsorbPolicy(projectId, next);
      if (res.status === "ok") {
        lastConfirmed = next;
      } else {
        absorbPolicy = prev;
      }
    }, 300);
  }
</script>

<fieldset class="absorb-policy">
  <legend>Chapter-divider silence</legend>
  {#each policies as p (p.value)}
    <label>
      <input
        type="radio"
        name={groupName}
        value={p.value}
        checked={absorbPolicy === p.value}
        onchange={() => onSelect(p.value)}
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
