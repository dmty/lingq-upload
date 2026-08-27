<script lang="ts">
  import { tick } from "svelte";

  import Button from "$lib/components/Button.svelte";

  /** Crop rectangle in 0..1 of the image, so it survives window resizes. */
  type Rect = { x: number; y: number; w: number; h: number };
  type Grip = "new" | "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

  const FULL: Rect = { x: 0, y: 0, w: 1, h: 1 };
  /** Smallest crop a drag can produce, in displayed px. */
  const MIN_DISPLAY_PX = 32;
  const EDGE_GRIPS: Grip[] = ["n", "s", "e", "w", "ne", "nw", "se", "sw"];

  let {
    open,
    src,
    title,
    sourceExt,
    onCancel,
    onSave,
    returnFocusTo = null,
  }: {
    open: boolean;
    src: string | null;
    title: string;
    /** Extension of the source image; decides the encoding on save. */
    sourceExt: string;
    onCancel: () => void;
    onSave: (blob: Blob, ext: string) => Promise<void>;
    returnFocusTo?: HTMLElement | null;
  } = $props();

  let dialog = $state<HTMLDialogElement | null>(null);
  let img = $state<HTMLImageElement | null>(null);
  let cropEl = $state<HTMLButtonElement | null>(null);

  let rect = $state<Rect>({ ...FULL });
  let natural = $state<{ w: number; h: number } | null>(null);
  let dragging = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  let grip: Grip | null = null;
  let origin = { x: 0, y: 0 };
  let start: Rect = { ...FULL };

  const outExt = $derived(sourceExt.toLowerCase() === "png" ? "png" : "jpg");
  const cropW = $derived(natural ? Math.round(rect.w * natural.w) : 0);
  const cropH = $derived(natural ? Math.round(rect.h * natural.h) : 0);
  // A full-frame "crop" would re-encode the image for nothing.
  const untouched = $derived(rect.w > 0.999 && rect.h > 0.999);

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) {
      rect = { ...FULL };
      error = null;
      dialog.showModal();
      void tick().then(() => cropEl?.focus());
    } else if (!open && dialog.open) {
      dialog.close();
    }
  });

  function cancel() {
    if (!busy) onCancel();
  }

  function onImageLoad(event: Event) {
    const el = event.currentTarget as HTMLImageElement;
    natural = { w: el.naturalWidth, h: el.naturalHeight };
  }

  // The sheet stays mounted while closed, so a cover can finish decoding long
  // before it is opened — in that case `load` has already been and gone.
  $effect(() => {
    void src;
    const el = img;
    natural =
      el?.complete && el.naturalWidth > 0
        ? { w: el.naturalWidth, h: el.naturalHeight }
        : null;
  });

  function box(): DOMRect | null {
    const b = img?.getBoundingClientRect();
    return b && b.width > 0 && b.height > 0 ? b : null;
  }

  function clamp01(v: number): number {
    return Math.min(1, Math.max(0, v));
  }

  function pointOf(event: PointerEvent, b: DOMRect) {
    return {
      x: clamp01((event.clientX - b.left) / b.width),
      y: clamp01((event.clientY - b.top) / b.height),
    };
  }

  /** Rect from two dragged corners, expanded to the minimum and kept in frame. */
  function fromCorners(
    ax: number,
    ay: number,
    bx: number,
    by: number,
    minW: number,
    minH: number,
  ): Rect {
    const w = Math.min(Math.max(Math.abs(bx - ax), minW), 1);
    const h = Math.min(Math.max(Math.abs(by - ay), minH), 1);
    return {
      x: Math.min(Math.max(0, Math.min(ax, bx)), 1 - w),
      y: Math.min(Math.max(0, Math.min(ay, by)), 1 - h),
      w,
      h,
    };
  }

  function onPointerDown(event: PointerEvent) {
    const b = box();
    if (!b || busy) return;
    const target = event.target as HTMLElement;
    grip = (target.dataset.grip as Grip | undefined) ?? "new";
    // A full-frame selection has nowhere to move to, so the first drag draws
    // a selection instead of dragging the whole image around.
    if (grip === "move" && untouched) grip = "new";
    origin = pointOf(event, b);
    start = { ...rect };
    dragging = true;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    event.preventDefault();
    cropEl?.focus();
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging || grip === null) return;
    const b = box();
    if (!b) return;
    const p = pointOf(event, b);
    const minW = Math.min(MIN_DISPLAY_PX / b.width, 1);
    const minH = Math.min(MIN_DISPLAY_PX / b.height, 1);

    if (grip === "new") {
      rect = fromCorners(origin.x, origin.y, p.x, p.y, minW, minH);
      return;
    }
    if (grip === "move") {
      rect = {
        ...start,
        x: Math.min(Math.max(0, start.x + (p.x - origin.x)), 1 - start.w),
        y: Math.min(Math.max(0, start.y + (p.y - origin.y)), 1 - start.h),
      };
      return;
    }
    // Each dragged edge stops at the minimum rather than crossing its opposite.
    let left = start.x;
    let top = start.y;
    let right = start.x + start.w;
    let bottom = start.y + start.h;
    if (grip.includes("w")) left = Math.min(p.x, right - minW);
    if (grip.includes("e")) right = Math.max(p.x, left + minW);
    if (grip.includes("n")) top = Math.min(p.y, bottom - minH);
    if (grip.includes("s")) bottom = Math.max(p.y, top + minH);
    rect = { x: left, y: top, w: right - left, h: bottom - top };
  }

  function endDrag(event: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    grip = null;
    (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
  }

  /** Arrows move the crop, Alt+arrows resize it — the pointer-free path. */
  function onKeydown(event: KeyboardEvent) {
    const b = box();
    if (!b) return;
    const step = event.shiftKey ? 10 : 1;
    const dx = step / b.width;
    const dy = step / b.height;
    const minW = Math.min(MIN_DISPLAY_PX / b.width, 1);
    const minH = Math.min(MIN_DISPLAY_PX / b.height, 1);
    let moveX = 0;
    let moveY = 0;
    switch (event.key) {
      case "ArrowLeft":
        moveX = -dx;
        break;
      case "ArrowRight":
        moveX = dx;
        break;
      case "ArrowUp":
        moveY = -dy;
        break;
      case "ArrowDown":
        moveY = dy;
        break;
      default:
        return;
    }
    event.preventDefault();
    if (event.altKey) {
      const w = Math.min(Math.max(rect.w + moveX, minW), 1 - rect.x);
      const h = Math.min(Math.max(rect.h + moveY, minH), 1 - rect.y);
      rect = { ...rect, w, h };
      return;
    }
    rect = {
      ...rect,
      x: Math.min(Math.max(0, rect.x + moveX), 1 - rect.w),
      y: Math.min(Math.max(0, rect.y + moveY), 1 - rect.h),
    };
  }

  function onDialogKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      cancel();
      return;
    }
    if (event.key !== "Tab" || !dialog) return;
    const focusable = Array.from(
      dialog.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    );
    const first = focusable[0];
    const last = focusable.at(-1);
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }

  async function save() {
    if (busy || !img || !natural) return;
    busy = true;
    error = null;
    try {
      const sx = Math.round(rect.x * natural.w);
      const sy = Math.round(rect.y * natural.h);
      const sw = Math.max(1, Math.round(rect.w * natural.w));
      const sh = Math.max(1, Math.round(rect.h * natural.h));
      const canvas = document.createElement("canvas");
      canvas.width = sw;
      canvas.height = sh;
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("This system has no 2D canvas.");
      if (outExt === "jpg") {
        // JPEG carries no alpha; without this a transparent source goes black.
        ctx.fillStyle = "#ffffff";
        ctx.fillRect(0, 0, sw, sh);
      }
      ctx.drawImage(img, sx, sy, sw, sh, 0, 0, sw, sh);
      const blob = await new Promise<Blob | null>((resolve) => {
        canvas.toBlob(
          resolve,
          outExt === "png" ? "image/png" : "image/jpeg",
          0.92,
        );
      });
      if (!blob) throw new Error("Could not encode the cropped image.");
      await onSave(blob, outExt);
    } catch (cause) {
      error =
        cause instanceof Error ? cause.message : "Could not save the cover.";
    } finally {
      busy = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="cover-editor-title"
  aria-modal="true"
  onkeydown={onDialogKeydown}
  oncancel={(event) => {
    event.preventDefault();
    cancel();
  }}
  onclose={() => returnFocusTo?.focus()}
>
  <div class="sheet-card flex flex-col gap-4">
    <header class="min-w-0">
      <p
        class="mb-1 text-[11px] font-medium uppercase tracking-[0.08em] text-accent"
      >
        Cover
      </p>
      <h2
        id="cover-editor-title"
        class="truncate font-serif text-lg font-semibold leading-snug text-fg"
      >
        {title}
      </h2>
    </header>

    <div
      class="well grid place-items-center overflow-hidden rounded-md border border-border bg-surface-sunken p-5"
    >
      {#if src}
        <figure class="frame relative m-0 leading-none shadow-card">
          <!-- crossorigin is load-bearing: without it WKWebView taints the
               canvas and the crop's toBlob throws SecurityError. Tauri's asset
               protocol answers with Access-Control-Allow-Origin, so the CORS
               load itself succeeds. -->
          <img
            bind:this={img}
            {src}
            data-testid="cover-image"
            alt="Cover for {title}"
            crossorigin="anonymous"
            onload={onImageLoad}
            onerror={() => (error = "Could not open the cover image.")}
            class="block h-auto w-auto max-w-full select-none"
            draggable="false"
          />
          {#if natural}
            <!-- Pointer target for the whole image: a press outside the crop
                 starts a fresh selection, one inside moves or resizes it. -->
            <div
              class="absolute inset-0 cursor-crosshair touch-none"
              onpointerdown={onPointerDown}
              onpointermove={onPointerMove}
              onpointerup={endDrag}
              onpointercancel={endDrag}
              role="presentation"
            >
              <button
                bind:this={cropEl}
                type="button"
                data-grip="move"
                data-testid="cover-crop-rect"
                aria-label="Crop area. Arrow keys move it, Alt with arrow keys resizes it."
                class="crop absolute cursor-move focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
                style="left:{rect.x * 100}%;top:{rect.y * 100}%;width:{rect.w *
                  100}%;height:{rect.h * 100}%"
                onkeydown={onKeydown}
              >
                <div
                  class="guides pointer-events-none absolute inset-0 transition-opacity duration-150 ease-snappy"
                  class:visible={dragging}
                ></div>
                {#each EDGE_GRIPS as edge (edge)}
                  <span class="grip grip-{edge}" data-grip={edge}></span>
                {/each}
                <span
                  class="readout pointer-events-none absolute tabular-nums"
                  data-testid="cover-crop-size"
                >
                  {cropW} × {cropH}
                </span>
              </button>
            </div>
          {/if}
        </figure>
      {:else}
        <p class="p-8 text-sm text-fg-muted">This project has no cover yet.</p>
      {/if}
    </div>

    {#if error}
      <p class="text-sm text-error" role="alert" aria-live="assertive">
        {error}
      </p>
    {/if}

    <footer class="flex items-center gap-3">
      <p class="min-w-0 flex-1 text-xs text-fg-subtle">
        {#if natural}
          Drag to select an area · original {natural.w} × {natural.h}
        {:else}
          Loading cover…
        {/if}
      </p>
      <button
        type="button"
        data-testid="cover-crop-reset"
        class="rounded-sm px-2 py-1 text-xs text-fg-muted transition-colors duration-120 ease-snappy hover:text-fg disabled:opacity-40"
        disabled={busy || untouched}
        onclick={() => (rect = { ...FULL })}
      >
        Reset
      </button>
      <Button variant="secondary" disabled={busy} onclick={cancel}>
        Cancel
      </Button>
      <Button
        data-testid="cover-crop-save"
        disabled={busy || untouched || natural === null}
        onclick={save}
      >
        {busy ? "Saving…" : "Save cover"}
      </Button>
    </footer>
  </div>
</dialog>

<style>
  /* px, not rem: exact pixel sizing, literal so it can't drift with the
     type scale — see src/app.css. */
  dialog {
    width: min(720px, calc(100vw - 32px));
  }

  /* The image sits in a sunken well the way an AppKit image well does, so a
     pale cover still reads as content rather than as sheet background. */
  .well {
    box-shadow: inset 0 1px 2px rgb(0 0 0 / 0.12);
  }

  /* Checkerboard behind the image: the only honest backdrop for a cover with
     transparency, and it makes the crop edges legible on white artwork. */
  .frame {
    --square: 8px;
    background-image:
      linear-gradient(45deg, rgb(128 128 128 / 0.18) 25%, transparent 25%),
      linear-gradient(-45deg, rgb(128 128 128 / 0.18) 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, rgb(128 128 128 / 0.18) 75%),
      linear-gradient(-45deg, transparent 75%, rgb(128 128 128 / 0.18) 75%);
    background-size: calc(var(--square) * 2) calc(var(--square) * 2);
    background-position:
      0 0,
      0 var(--square),
      var(--square) calc(var(--square) * -1),
      calc(var(--square) * -1) 0;
  }

  .frame img {
    max-height: min(52vh, 460px);
  }

  /* One box-shadow dims everything outside the crop; the parent clips it to
     the image, so no separate scrim elements are needed. */
  .crop {
    box-shadow:
      0 0 0 9999px rgb(0 0 0 / 0.55),
      inset 0 0 0 1px rgb(255 255 255 / 0.9),
      inset 0 0 0 2px rgb(0 0 0 / 0.35);
  }

  /* Rule-of-thirds, only while the crop is being worked — a composition aid
     during the drag, not permanent furniture over the artwork. */
  .guides {
    opacity: 0;
    background-image:
      linear-gradient(
        to right,
        transparent calc(33.333% - 0.5px),
        rgb(255 255 255 / 0.4) calc(33.333% - 0.5px),
        rgb(255 255 255 / 0.4) calc(33.333% + 0.5px),
        transparent calc(33.333% + 0.5px),
        transparent calc(66.667% - 0.5px),
        rgb(255 255 255 / 0.4) calc(66.667% - 0.5px),
        rgb(255 255 255 / 0.4) calc(66.667% + 0.5px),
        transparent calc(66.667% + 0.5px)
      ),
      linear-gradient(
        to bottom,
        transparent calc(33.333% - 0.5px),
        rgb(255 255 255 / 0.4) calc(33.333% - 0.5px),
        rgb(255 255 255 / 0.4) calc(33.333% + 0.5px),
        transparent calc(33.333% + 0.5px),
        transparent calc(66.667% - 0.5px),
        rgb(255 255 255 / 0.4) calc(66.667% - 0.5px),
        rgb(255 255 255 / 0.4) calc(66.667% + 0.5px),
        transparent calc(66.667% + 0.5px)
      );
  }

  .guides.visible {
    opacity: 1;
  }

  /* Square knobs with a hairline, the way AppKit draws selection handles —
     round dots would read as a web crop widget. */
  .grip {
    position: absolute;
    width: 10px;
    height: 10px;
    background: #fff;
    border: 1px solid rgb(0 0 0 / 0.45);
    border-radius: 1px;
    box-shadow: 0 1px 1px rgb(0 0 0 / 0.3);
  }

  .grip-n,
  .grip-s {
    left: 50%;
    margin-left: -5px;
    cursor: ns-resize;
  }

  .grip-e,
  .grip-w {
    top: 50%;
    margin-top: -5px;
    cursor: ew-resize;
  }

  .grip-n,
  .grip-ne,
  .grip-nw {
    top: -5px;
  }

  .grip-s,
  .grip-se,
  .grip-sw {
    bottom: -5px;
  }

  .grip-w,
  .grip-nw,
  .grip-sw {
    left: -5px;
  }

  .grip-e,
  .grip-ne,
  .grip-se {
    right: -5px;
  }

  .grip-nw,
  .grip-se {
    cursor: nwse-resize;
  }

  .grip-ne,
  .grip-sw {
    cursor: nesw-resize;
  }

  .readout {
    bottom: 6px;
    left: 6px;
    padding: 1px 6px;
    font-size: 10px;
    color: #fff;
    background: rgb(0 0 0 / 0.62);
    border-radius: 3px;
    -webkit-backdrop-filter: blur(4px);
    backdrop-filter: blur(4px);
  }
</style>
