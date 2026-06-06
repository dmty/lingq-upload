<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { commands, type MismatchResponse } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import MismatchEvidence from "$lib/components/MismatchEvidence.svelte";
  import ResponseCard from "$lib/components/ResponseCard.svelte";

  type Q = {
    title?: string;
    chapters?: string;
    tracks?: string;
    condition?: "one_to_many" | "many_to_one" | "count_off" | "unalignable";
    options?: string;
    preselect?: MismatchResponse;
  };

  const params = $derived<Q>(Object.fromEntries(page.url.searchParams));
  const title = $derived(params.title ?? "Untitled");
  const chapters = $derived(Number(params.chapters ?? "0"));
  const tracks = $derived(Number(params.tracks ?? "0"));
  const condition = $derived(params.condition ?? "count_off");
  const options = $derived<MismatchResponse[]>(
    (params.options?.split(",") as MismatchResponse[]) ?? ["cancel"],
  );

  let selected = $state<MismatchResponse>(
    (page.url.searchParams.get("preselect") as MismatchResponse) ?? "cancel",
  );
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function confirm() {
    busy = true;
    error = null;
    if (selected === "cancel") {
      goto("/library");
      return;
    }
    const projectKey = page.params.projectId;
    const loaded = await commands.cmdProjectLoad(projectKey);
    if (loaded.status === "error") {
      error = appErrorMessage(loaded.error);
      busy = false;
      return;
    }
    const resolved = await commands.cmdMatcherResolve(
      loaded.data.id,
      condition,
      selected,
      chapters,
      tracks,
    );
    if (resolved.status === "error") {
      error = appErrorMessage(resolved.error);
      busy = false;
      return;
    }
    goto(`/run/${projectKey}`);
  }
</script>

<section class="mx-auto max-w-2xl space-y-6 pt-6">
  <header>
    <h1 class="text-lg font-semibold text-fg">Resolve mismatch</h1>
  </header>

  <MismatchEvidence {title} {chapters} {tracks} {condition} />

  <div class="space-y-2">
    {#each options as opt (opt)}
      <ResponseCard
        response={opt}
        selected={selected === opt}
        onSelect={() => (selected = opt)}
      />
    {/each}
  </div>

  {#if error}
    <p
      class="rounded-sm border border-error-soft bg-error-soft/30 px-4 py-2 text-sm"
    >
      {error}
    </p>
  {/if}

  <div class="flex justify-end gap-2">
    <a
      href="/library"
      class="rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg hover:bg-surface-sunken"
    >
      Back
    </a>
    <button
      type="button"
      onclick={confirm}
      disabled={busy}
      class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
    >
      Confirm
    </button>
  </div>
</section>
