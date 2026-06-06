<script lang="ts">
  import type { UploadResult } from "$lib/ipc/bindings";

  interface Props {
    title: string;
    result: UploadResult;
    onUploadAnother: () => void;
  }

  let { title, result, onUploadAnother }: Props = $props();
</script>

<div class="p-6">
  <div class="flex items-start gap-3">
    <span
      class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-success-soft text-success"
      aria-hidden="true"
    >
      <svg
        width="16"
        height="16"
        viewBox="0 0 20 20"
        fill="none"
        stroke="currentColor"
        stroke-width="2.5"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M4 10.5l4 4 8-9" />
      </svg>
    </span>
    <div class="flex-1">
      <h2 class="text-md font-semibold text-fg">
        Lesson added to your library.
      </h2>
      <p class="mt-1 text-sm text-fg-muted">
        {title || "Untitled lesson"} · ID
        <code class="tabular text-fg">{result.lesson_id}</code>
      </p>
      <div class="mt-4 flex items-center gap-2">
        <a
          href={result.lesson_url}
          target="_blank"
          rel="noopener noreferrer"
          class="inline-flex h-9 items-center gap-2 rounded-sm bg-accent px-4 text-sm font-medium text-white no-underline transition-colors duration-180 ease-snappy hover:bg-accent-hover hover:no-underline"
        >
          Open in LingQ
          <svg
            width="13"
            height="13"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M7 17L17 7" />
            <path d="M8 7h9v9" />
          </svg>
        </a>
        <button
          type="button"
          onclick={onUploadAnother}
          class="rounded-sm px-3 py-2 text-sm font-medium text-fg-muted transition-colors duration-120 hover:bg-surface-sunken hover:text-fg"
        >
          Upload another
        </button>
      </div>
    </div>
  </div>
</div>
