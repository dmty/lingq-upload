import { expect, seed, test } from "./setup/test";
import { chapters, installMapping, pair } from "./setup/mapping-fixture";
import type { Page } from "@playwright/test";

import type { BucketMeta, MismatchInspection } from "../src/lib/ipc/bindings";

const PROJECT_KEY = "cover-editor-fixture";
const COVER_URL = "http://cover.test/botchan.png";
const COVER_W = 200;
const COVER_H = 300;

// A flat 200x300 PNG. Served through page.route so the editor's
// crossorigin="anonymous" image really is a CORS-approved cross-origin load —
// the same condition the Tauri asset protocol creates, and the one that
// decides whether the crop canvas can be read back.
const COVER_PNG_BASE64 =
  "iVBORw0KGgoAAAANSUhEUgAAAMgAAAEsCAIAAAAJmGvpAAACI0lEQVR42u3SQQkAAAgEwUtnHNMZ0BKCn4FJsGyqB85FAoyFsTAWGAtjYSwwFsbCWGAsjIWxwFgYC2OBsTAWxgJjYSyMBcbCWBgLjIWxMBYYC2NhLDAWxsJYYCyMhbHAWBgLY4GxMBbGAmNhLIwFxsJYGAuMhbEwFhgLY2EsMBbGwlhgLIyFscBYGAtjgbEwFsYCY2EsjAXGwlgYC4yFsTAWGAtjYSwwFsbCWGAsjIWxwFgYC2NhLBUwFsbCWGAsjIWxwFgYC2OBsTAWxgJjYSyMBcbCWBgLjIWxMBYYC2NhLDAWxsJYYCyMhbHAWBgLY4GxMBbGAmNhLIwFxsJYGAuMhbEwFhgLY2EsMBbGwlhgLIyFscBYGAtjgbEwFsYCY2EsjAXGwlgYC4yFsTAWGAtjYSwwFsbCWGAsjIWxwFgYC2OBsTAWxsJYKmAsjIWxwFgYC2OBsTAWxgJjYSyMBcbCWBgLjIWxMBYYC2NhLDAWxsJYYCyMhbHAWBgLY4GxMBbGAmNhLIwFxsJYGAuMhbEwFhgLY2EsMBbGwlhgLIyFscBYGAtjgbEwFsYCY2EsjAXGwlgYC4yFsTAWGAtjYSwwFsbCWGAsjIWxwFgYC2OBsTAWxgJjYSyMhbEkwFgYC2OBsTAWxgJjYSyMBcbCWBgLjIWxMBYYC2NhLDAWxsJYYCyMhbHAWBgLY4GxMBbGAmNhLIwFxsJYGAuMhbEwFhiLfwumEKpI8nUsMAAAAABJRU5ErkJggg==";

async function coverBox(page: Page) {
  // The sheet slides down from the titlebar; hover waits for it to settle so
  // a drag measured here does not land where the sheet used to be.
  await page.getByTestId("cover-crop-rect").hover();
  const box = await page.getByTestId("cover-image").boundingBox();
  if (!box) throw new Error("cover image has no layout box");
  return box;
}

async function cropRatio(page: Page) {
  const text = (await page.getByTestId("cover-crop-size").textContent()) ?? "";
  const [w, h] = text.split("×").map((n) => Number(n.trim()));
  return w / h;
}

const projectChapters = chapters(4, 50);

const buckets: BucketMeta[] = [
  {
    trackId: "t0",
    atomTitle: "Audio 1",
    atomDurationSec: 600,
    charsPerSec: 5,
    audioPath: "/audio/t0.m4a",
    window: [0, 600],
  },
];

const mapping = {
  pairs: projectChapters.map((c, i) => pair(i, "t0", 1)),
  parking_lot: [],
  op_id: 0,
  buckets,
};

const inspection: MismatchInspection = {
  title: "Botchan",
  chapter_count: 4,
  track_count: 1,
  condition: "many_to_few",
  options: ["split_proportional", "cancel"],
  preselect: "split_proportional",
  bucket_preview: null,
};

test.describe("cover editor", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("http://cover.test/**", (route) =>
      route.fulfill({
        status: 200,
        contentType: "image/png",
        headers: { "Access-Control-Allow-Origin": "*" },
        body: Buffer.from(COVER_PNG_BASE64, "base64"),
      }),
    );
    await installMapping(page, {
      key: PROJECT_KEY,
      chapters: projectChapters,
      mapping,
      inspection,
    });
    await seed(page, {
      __projectMeta__: {
        [PROJECT_KEY]: {
          title: "Botchan",
          authors: ["Natsume Soseki"],
          cover_path: COVER_URL,
        },
      },
    });
    await page.goto(`/match/${PROJECT_KEY}`);
    await expect(page.getByTestId("mapping-grid")).toBeVisible();
  });

  test("opens the cover full size with the whole image selected", async ({
    page,
  }) => {
    await page.getByTestId("cover-open-editor").click();

    await expect(page.getByTestId("cover-image")).toBeVisible();
    // Shown at full size, not as the 64px header thumb.
    const shown = await coverBox(page);
    expect(shown.width).toBeCloseTo(COVER_W, 0);
    expect(shown.height).toBeCloseTo(COVER_H, 0);
    await expect(page.getByTestId("cover-crop-size")).toHaveText(
      `${COVER_W} × ${COVER_H}`,
    );
    // Nothing cropped yet, so there is nothing to save or reset.
    await expect(page.getByTestId("cover-crop-save")).toBeDisabled();
    await expect(page.getByTestId("cover-crop-reset")).toBeDisabled();
  });

  test("dragging an area crops it and saves the encoded region", async ({
    page,
  }) => {
    await page.getByTestId("cover-open-editor").click();
    const box = await coverBox(page);

    await page.mouse.move(box.x + box.width * 0.25, box.y + box.height * 0.25);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.75, box.y + box.height * 0.75, {
      steps: 8,
    });
    await page.mouse.up();

    // Half the width and half the height of the source, in source pixels.
    await expect(page.getByTestId("cover-crop-size")).toHaveText(
      `${COVER_W / 2} × ${COVER_H / 2}`,
    );

    await page.getByTestId("cover-crop-save").click();

    await expect(page.getByTestId("cover-image")).toBeHidden();
    const saved = await page.evaluate(() => window.__savedCover__);
    // A PNG source stays a PNG, and the canvas really did read back — a
    // tainted canvas would have thrown instead of producing bytes.
    expect(saved?.ext).toBe("png");
    expect(saved?.byteLength ?? 0).toBeGreaterThan(0);
  });

  test("reset restores the full frame", async ({ page }) => {
    await page.getByTestId("cover-open-editor").click();
    const box = await coverBox(page);

    await page.mouse.move(box.x + box.width * 0.3, box.y + box.height * 0.3);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.6, {
      steps: 4,
    });
    await page.mouse.up();
    await expect(page.getByTestId("cover-crop-size")).not.toHaveText(
      `${COVER_W} × ${COVER_H}`,
    );

    await page.getByTestId("cover-crop-reset").click();

    await expect(page.getByTestId("cover-crop-size")).toHaveText(
      `${COVER_W} × ${COVER_H}`,
    );
  });

  test("arrow keys move the crop without a pointer", async ({ page }) => {
    await page.getByTestId("cover-open-editor").click();
    const box = await coverBox(page);

    await page.mouse.move(box.x + box.width * 0.25, box.y + box.height * 0.25);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.75, box.y + box.height * 0.75, {
      steps: 4,
    });
    await page.mouse.up();

    const crop = page.getByTestId("cover-crop-rect");
    await expect(crop).toBeFocused();
    const before = await crop.boundingBox();
    await page.keyboard.press("Shift+ArrowLeft");
    const after = await crop.boundingBox();

    expect((before?.x ?? 0) - (after?.x ?? 0)).toBeCloseTo(10, 0);
    // Moving must not resize.
    await expect(page.getByTestId("cover-crop-size")).toHaveText(
      `${COVER_W / 2} × ${COVER_H / 2}`,
    );
  });

  test("square selects the biggest centred square", async ({ page }) => {
    await page.getByTestId("cover-open-editor").click();
    await coverBox(page);

    await page.getByTestId("cover-crop-square").click();

    // The short side of a 200x300 cover.
    await expect(page.getByTestId("cover-crop-size")).toHaveText(
      `${COVER_W} × ${COVER_W}`,
    );
    // The readout is computed from the source pixels, so check the drawn
    // selection is square too — that is what the user is looking at.
    const drawn = await page.getByTestId("cover-crop-rect").boundingBox();
    expect(drawn?.width).toBeCloseTo(drawn?.height ?? 0, 0);
  });

  test("shift keeps the shape while dragging a handle", async ({ page }) => {
    await page.getByTestId("cover-open-editor").click();
    const box = await coverBox(page);
    await page.getByTestId("cover-crop-square").click();

    const grip = await page.locator('[data-grip="se"]').boundingBox();
    if (!grip) throw new Error("no south-east grip");
    await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2);
    await page.keyboard.down("Shift");
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.4, {
      steps: 8,
    });
    await page.mouse.up();
    await page.keyboard.up("Shift");

    // Smaller, but still square — without Shift this drag gives 120 x 70.
    await expect(page.getByTestId("cover-crop-size")).toHaveText("120 × 120");
  });

  test("shift on a side handle resizes the other axis too", async ({
    page,
  }) => {
    await page.getByTestId("cover-open-editor").click();
    const box = await coverBox(page);
    await page.getByTestId("cover-crop-square").click();

    const grip = await page.locator('[data-grip="e"]').boundingBox();
    if (!grip) throw new Error("no east grip");
    await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2);
    await page.keyboard.down("Shift");
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.5, grip.y + grip.height / 2, {
      steps: 8,
    });
    await page.mouse.up();
    await page.keyboard.up("Shift");

    // Only the east edge moved, but the height follows so the square holds.
    await expect(page.getByTestId("cover-crop-size")).toHaveText("100 × 100");
  });

  test("shift-drag slides a cornered selection back into frame", async ({
    page,
  }) => {
    await page.getByTestId("cover-open-editor").click();
    const box = await coverBox(page);

    // A small selection near the top-left: growing it about its own centre
    // would push it off the left edge. Started clear of the full-frame
    // selection's own corner grip, which would resize instead of draw.
    await page.mouse.move(box.x + 20, box.y + 20);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.3, box.y + box.height * 0.19, {
      steps: 4,
    });
    await page.mouse.up();
    const ratio = await cropRatio(page);
    const small = await page.getByTestId("cover-crop-rect").boundingBox();

    const grip = await page.locator('[data-grip="s"]').boundingBox();
    if (!grip) throw new Error("no south grip");
    await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2);
    await page.keyboard.down("Shift");
    await page.mouse.down();
    await page.mouse.move(grip.x + grip.width / 2, box.y + box.height * 0.6, {
      steps: 8,
    });
    await page.mouse.up();
    await page.keyboard.up("Shift");

    const grown = await page.getByTestId("cover-crop-rect").boundingBox();
    expect(grown?.width ?? 0).toBeGreaterThan(small?.width ?? 0);
    expect(await cropRatio(page)).toBeCloseTo(ratio, 1);
    // Slid right rather than hanging off the left edge of the image.
    expect(grown?.x ?? 0).toBeGreaterThanOrEqual(box.x - 1);
    expect((grown?.x ?? 0) + (grown?.width ?? 0)).toBeLessThanOrEqual(
      box.x + box.width + 1,
    );
  });

  test("escape closes the sheet and returns focus to the cover", async ({
    page,
  }) => {
    await page.getByTestId("cover-open-editor").click();
    await expect(page.getByTestId("cover-image")).toBeVisible();

    await page.keyboard.press("Escape");

    await expect(page.getByTestId("cover-image")).toBeHidden();
    await expect(page.getByTestId("cover-open-editor")).toBeFocused();
  });
});
