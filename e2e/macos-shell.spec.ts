import { expect, test } from "@playwright/test";
import { tauriStubInitScriptFor } from "./setup/tauri-stub";

test.describe("macOS shell tokens", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
  });

  test("body sits on the macOS 13px base size", async ({ page }) => {
    const size = await page.evaluate(
      () => getComputedStyle(document.body).fontSize,
    );
    expect(size).toBe("13px");
  });

  // html must stay at the browser default so 1rem is 16px for Tailwind's
  // spacing scale — regressing this shrinks every rem-based utility ~19%.
  test("html stays at the 16px default so 1rem drives the utility scale", async ({
    page,
  }) => {
    const size = await page.evaluate(
      () => getComputedStyle(document.documentElement).fontSize,
    );
    expect(size).toBe("16px");
  });

  test("accent fills resolve to a real colour, not a literal token", async ({
    page,
  }) => {
    const probe = await page.evaluate(() => {
      const el = document.createElement("button");
      el.className = "bg-accent text-accent-fg";
      document.body.append(el);
      const cs = getComputedStyle(el);
      const out = { bg: cs.backgroundColor, fg: cs.color };
      el.remove();
      return out;
    });
    expect(probe.bg).toMatch(/^rgba?\(/);
    expect(probe.bg).not.toBe("rgba(0, 0, 0, 0)");
    expect(probe.bg).not.toBe("rgb(79, 70, 229)"); // the old hardcoded indigo
    expect(probe.fg).toMatch(/^rgba?\(/);
    expect(probe.fg).not.toBe(probe.bg);
  });
});

test.describe("source list sidebar", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("all four sections live in a labelled sidebar nav", async ({ page }) => {
    await page.goto("/library");
    const nav = page.getByRole("navigation", { name: "Sections" });
    await expect(nav).toBeVisible();
    for (const name of ["Library", "Add", "Quick upload", "Settings"]) {
      await expect(nav.getByRole("link", { name, exact: true })).toBeVisible();
    }
  });

  test("the sidebar sits beside the content, not above it", async ({ page }) => {
    await page.goto("/library");
    const nav = await page
      .getByRole("navigation", { name: "Sections" })
      .boundingBox();
    const main = await page.locator("main").boundingBox();
    expect(nav).not.toBeNull();
    expect(main).not.toBeNull();
    expect(main!.x).toBeGreaterThanOrEqual(nav!.x + nav!.width);
  });

  test("the current section is marked for assistive tech", async ({ page }) => {
    await page.goto("/settings");
    const nav = page.getByRole("navigation", { name: "Sections" });
    await expect(nav.getByRole("link", { name: "Settings", exact: true })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(nav.getByRole("link", { name: "Library", exact: true })).not.toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  test("content scrolls while the sidebar stays put", async ({ page }) => {
    await page.goto("/settings");
    await page.waitForLoadState("networkidle");
    const nav = page.getByRole("navigation", { name: "Sections" });
    const before = await nav.boundingBox();
    await page.locator("main").evaluate((el) => {
      el.style.height = "200px";
      el.scrollTop = 400;
    });
    const scrollTop = await page.locator("main").evaluate((el) => el.scrollTop);
    expect(scrollTop).toBeGreaterThan(0);
    const after = await nav.boundingBox();
    expect(before).not.toBeNull();
    expect(after!.y).toBe(before!.y);
  });

  test("the scroll hairline only appears once content has scrolled", async ({
    page,
  }) => {
    await page.goto("/settings");
    await page.waitForLoadState("networkidle");
    const main = page.locator("main");
    expect(await main.evaluate((el) => getComputedStyle(el).borderTopColor)).toBe(
      "rgba(0, 0, 0, 0)",
    );
    await main.evaluate((el) => {
      el.style.height = "200px";
      el.scrollTop = 400;
      el.dispatchEvent(new Event("scroll"));
    });
    await page.waitForTimeout(150); // let the 120ms border-color transition settle
    expect(
      await main.evaluate((el) => getComputedStyle(el).borderTopColor),
    ).not.toBe("rgba(0, 0, 0, 0)");
  });
});

test.describe("overlay titlebar", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("a drag strip reserves room above the sidebar sections", async ({
    page,
  }) => {
    await page.goto("/library");
    const strip = page.locator("[data-tauri-drag-region]").first();
    await expect(strip).toBeVisible();
    const box = await strip.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.y).toBe(0);
    expect(box!.height).toBeGreaterThanOrEqual(44);
  });

  test("the first section row clears the traffic lights", async ({ page }) => {
    await page.goto("/library");
    const first = page
      .getByRole("navigation", { name: "Sections" })
      .getByRole("link", { name: "Library", exact: true });
    const box = await first.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.y).toBeGreaterThanOrEqual(44);
  });

  test("the document is transparent so vibrancy can show through", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const bg = await page.evaluate(
      () => getComputedStyle(document.body).backgroundColor,
    );
    expect(bg).toBe("rgba(0, 0, 0, 0)");
  });

  // The sidebar strip alone left the band above the content pane undraggable,
  // which is the half of the titlebar the pointer actually lands on.
  test("the content pane's top band is draggable too", async ({ page }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const strips = page.locator("[data-tauri-drag-region]");
    await expect(strips).toHaveCount(2);
    const main = await page.locator("main").boundingBox();
    const strip = await page.locator(".titlebar-drag").boundingBox();
    expect(strip).not.toBeNull();
    expect(strip!.y).toBe(0);
    expect(strip!.x).toBe(main!.x);
    expect(strip!.width).toBe(main!.width);
  });

  // Any taller and it covers the first heading; any shorter and there is a
  // dead strip of titlebar. Both edges are checked against main's own inset.
  test("the content drag strip stops exactly where content begins", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const inset = await page.locator("main").evaluate((el) => {
      const cs = getComputedStyle(el);
      return parseFloat(cs.borderTopWidth) + parseFloat(cs.paddingTop);
    });
    const strip = await page.locator(".titlebar-drag").boundingBox();
    expect(strip!.height).toBe(inset);
    const heading = await page
      .getByRole("heading", { name: "Library" })
      .boundingBox();
    expect(heading!.y).toBeGreaterThanOrEqual(strip!.height);
  });
});

test.describe("text selection", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    // Without entries the library renders its empty state, which has no
    // search field for the probe below to read.
    await page.addInitScript(statusEntriesScript());
  });

  test("chrome is unselectable but fields and alerts are not", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const probe = await page.evaluate(() => {
      const read = (el: Element) => getComputedStyle(el).userSelect;
      const alert = document.createElement("div");
      alert.setAttribute("role", "alert");
      document.body.append(alert);
      const out = {
        body: read(document.body),
        heading: read(document.querySelector("h1")!),
        nav: read(document.querySelector('a[href="/library"]')!),
        input: read(document.querySelector('input[type="search"]')!),
        alert: read(alert),
      };
      alert.remove();
      return out;
    });
    expect(probe.body).toBe("none");
    expect(probe.heading).toBe("none");
    expect(probe.nav).toBe("none");
    expect(probe.input).toBe("text");
    expect(probe.alert).toBe("text");
  });
});

test.describe("form controls", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    // The search field and popup button only exist on a non-empty library.
    await page.addInitScript(statusEntriesScript());
  });

  test("every text input carries the shared field chrome", async ({ page }) => {
    await page.goto("/settings");
    await page.waitForLoadState("networkidle");
    const inputs = page.locator('input[type="text"], input[type="password"], select');
    const count = await inputs.count();
    expect(count).toBeGreaterThan(0);
    for (let i = 0; i < count; i += 1) {
      await expect(inputs.nth(i)).toHaveClass(/\bfield(-lg)?\b/);
    }
  });

  test("focusing a field paints an accent ring", async ({ page }) => {
    await page.goto("/settings");
    await page.waitForLoadState("networkidle");
    const field = page.locator("input.field, input.field-lg").first();
    await field.focus();
    await page.waitForTimeout(150); // let the 120ms box-shadow transition settle
    const shadow = await field.evaluate(
      (el) => getComputedStyle(el).boxShadow,
    );
    expect(shadow).not.toBe("none");
    expect(shadow).toContain("3.5px");
  });

  test("the search field is a 28px control with room for its glass", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const input = page.locator('input[type="search"]');
    const box = await input.boundingBox();
    expect(box!.height).toBe(28);
    const glass = await page
      .locator('input[type="search"] ~ svg, svg + input[type="search"]')
      .count();
    expect(glass).toBeGreaterThan(0);
    // The icon is absolutely positioned, so only the padding keeps the
    // caret and placeholder clear of it.
    const padLeft = await input.evaluate((el) =>
      parseFloat(getComputedStyle(el).paddingLeft),
    );
    expect(padLeft).toBeGreaterThanOrEqual(24);
  });

  // `appearance: none` is what makes the height above stick, and it also
  // strips WebKit's built-in ⊗ — so the field has to bring its own.
  test("the search field clears itself without touching the filter", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const clear = page.getByRole("button", { name: "Clear search" });
    await expect(clear).toBeHidden();

    const input = page.locator('input[type="search"]');
    await input.fill("tolstoy");
    await page.locator("select.field").first().selectOption("en");
    await expect(clear).toBeVisible();

    await clear.click();
    await expect(input).toHaveValue("");
    await expect(page.locator("select.field").first()).toHaveValue("en");
    await expect(input).toBeFocused();
  });

  test("a popup button wears an accent badge that dims with the window", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const badge = page.locator(".select-badge").first();
    await expect(badge).toBeVisible();

    const active = await badge.evaluate(
      (el) => getComputedStyle(el).backgroundColor,
    );
    const accent = await badge.evaluate((el) =>
      getComputedStyle(el).getPropertyValue("--color-accent").trim(),
    );
    expect(active).not.toBe("rgba(0, 0, 0, 0)");
    expect(accent).not.toBe("");

    // The badge must sit inside the control, not beside it.
    const select = await page.locator("select.field").first().boundingBox();
    const box = await badge.boundingBox();
    expect(box!.x).toBeGreaterThan(select!.x);
    expect(box!.x + box!.width).toBeLessThanOrEqual(select!.x + select!.width);

    await page.evaluate(() =>
      document.documentElement.setAttribute("data-window-inactive", ""),
    );
    await page.waitForTimeout(150); // let the 120ms background transition settle
    const inactive = await badge.evaluate(
      (el) => getComputedStyle(el).backgroundColor,
    );
    expect(inactive).not.toBe(active);
  });

  test("the popup button leaves room for its badge", async ({ page }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const padEnd = await page
      .locator("select.field")
      .first()
      .evaluate((el) => parseFloat(getComputedStyle(el).paddingRight));
    expect(padEnd).toBeGreaterThanOrEqual(24);
  });

  // The popup button's raised edge is also a box-shadow, so it can silently
  // swallow the focus ring they share the property with.
  test("focusing a popup button still paints the accent ring", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const select = page.locator("select.field").first();
    await select.focus();
    await page.waitForTimeout(150); // let the 120ms box-shadow transition settle
    const shadow = await select.evaluate(
      (el) => getComputedStyle(el).boxShadow,
    );
    expect(shadow).toContain("3.5px");
  });
});


// Minimal single-project fixture: just enough for the evidence panel's
// "Reset detected range" secondary button to render without a click, so the
// push-button assertion targets a real DOM node rather than a synthetic one.
function pushButtonFixtureScript(key: string): string {
  const chapters = [
    { id: "spine:start", order: 0, title: "Start", body: "", kind: "body" },
    { id: "spine:end", order: 1, title: "End", body: "", kind: "body" },
  ];
  const detection = {
    provider_id: "groq",
    align_source: "transcript",
    range: { start_chapter_id: "spine:start", end_chapter_id: "spine:end" },
    confidence: 0.9,
    transcript_head_preview: null,
    transcript_tail_preview: null,
    detected_at: "2026-08-16T00:00:00Z",
  };
  const mapping = {
    pairs: [
      {
        chapter_id: "spine:start",
        track_id: "t0",
        confidence: 1,
        original_confidence: 1,
        touched: false,
      },
      {
        chapter_id: "spine:end",
        track_id: "t1",
        confidence: 1,
        original_confidence: 1,
        touched: false,
      },
    ],
    parking_lot: [],
    op_id: 0,
  };
  return `;(() => {
    const key = ${JSON.stringify(key)};
    window.__pickerState__.chaptersByProject[key] = ${JSON.stringify(chapters)};
    window.__matcherDecisionByProject__ = {
      [key]: {
        condition: "many_to_few",
        response: "split_proportional",
        chapter_count: ${chapters.length},
        track_count: 2,
        user_overrode: false,
        decided_at: ${JSON.stringify(detection.detected_at)},
        detection: ${JSON.stringify(detection)},
      },
    };
    window.__mappingState__.seed(key, ${JSON.stringify(mapping)});
  })();`;
}

function statusEntriesScript(): string {
  const entry = {
    id: { content_hash: "book-1", audible_asin: null, isbn13: null, calibre_uuid: null },
    title: "War and Peace",
    authors: ["Tolstoy"],
    language: "en",
    completed_lesson_count: 0,
    receipt_count: 0,
    mtime: null,
    status: "needs_match",
    cover_path: null,
    last_activity_at: null,
    lingq_collection_id: null,
  };
  return `window.__libraryEntries__ = ${JSON.stringify([entry])};`;
}

test.describe("AppKit list and status treatment", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
    await page.addInitScript(statusEntriesScript());
  });

  test("the selected row is accent-filled, not tinted", async ({ page }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    // Selection is keyboard-driven; no row is aria-selected until focused.
    await page.keyboard.press("ArrowDown");
    const row = page.locator('[aria-selected="true"]').first();
    await expect(row).toBeVisible();
    const [bg, accent] = await row.evaluate((el) => [
      getComputedStyle(el).backgroundColor,
      getComputedStyle(el).getPropertyValue("--color-accent").trim(),
    ]);
    expect(bg).not.toBe("rgba(0, 0, 0, 0)");
    // accent-soft was a tint; the fill must now be the accent itself
    const soft = await row.evaluate((el) =>
      getComputedStyle(el).getPropertyValue("--color-accent-soft").trim(),
    );
    expect(accent).not.toBe("");
    expect(soft).not.toBe(bg);
  });

  test("status reads as a dot plus a plain label, not a pill", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const badge = page.getByTitle(/Mapping not confirmed/).first();
    await expect(badge).toBeVisible();
    await expect(badge).toContainText(/\S/);
    const bg = await badge.evaluate(
      (el) => getComputedStyle(el).backgroundColor,
    );
    expect(bg).toBe("rgba(0, 0, 0, 0)");
  });

  test("every label on the selected row stays legible on the fill", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    await page.keyboard.press("ArrowDown");
    const worst = await page.evaluate(() => {
      const row = document.querySelector('[aria-selected="true"]')!;
      const lum = (c: string) => {
        const m = c.match(/[\d.]+/g)!.map(Number);
        // color-mix() computes here, and Chromium serializes that as the
        // modern color(srgb r g b / a) function (0-1 per channel) rather
        // than legacy rgb()/rgba() (0-255) — scale must match the format.
        const scale = c.startsWith("color(") ? 1 : 255;
        const f = (v: number) => {
          const s = v / scale;
          return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
        };
        return 0.2126 * f(m[0]) + 0.7152 * f(m[1]) + 0.0722 * f(m[2]);
      };
      const fill = lum(getComputedStyle(row).backgroundColor);
      let min = Infinity;
      row.querySelectorAll("*").forEach((el) => {
        if (el.children.length > 0) return;
        if (!(el.textContent || "").trim()) return;
        if (el.closest(".cover-placeholder")) return; // has its own background
        const [a, b] = [lum(getComputedStyle(el).color), fill].sort(
          (p, q) => q - p,
        );
        min = Math.min(min, (a + 0.05) / (b + 0.05));
      });
      return min;
    });
    expect(worst).toBeGreaterThan(4.5);
  });

  test("hovering the selected row's own controls keeps the title legible", async ({
    page,
  }) => {
    for (const colorScheme of ["light", "dark"] as const) {
      await page.emulateMedia({ colorScheme });
      await page.goto("/library");
      await page.waitForLoadState("networkidle");
      await page.keyboard.press("ArrowDown");
      const row = page.locator('[aria-selected="true"]').first();
      await expect(row).toBeVisible();
      // .hover-through: this is the row's own button, whose hover would
      // otherwise paint over the accent fill and strand the white title.
      await row.getByRole("button", { name: /^Open/ }).hover();
      await page.waitForTimeout(250); // let transition-colors settle
      const ratio = await row.evaluate((el) => {
        const composite = (c: string) => {
          const cvs = document.createElement("canvas");
          cvs.width = cvs.height = 1;
          const ctx = cvs.getContext("2d")!;
          ctx.fillStyle = "white";
          ctx.fillRect(0, 0, 1, 1);
          ctx.fillStyle = c;
          ctx.fillRect(0, 0, 1, 1);
          return Array.from(ctx.getImageData(0, 0, 1, 1).data);
        };
        const lum = (c: string) => {
          const [r, g, b] = composite(c);
          const f = (v: number) => {
            const s = v / 255;
            return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
          };
          return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
        };
        const fill = lum(getComputedStyle(el).backgroundColor);
        const title = el.querySelector('[data-testid="library-title"]')!;
        const [a, b] = [lum(getComputedStyle(title).color), fill].sort(
          (p, q) => q - p,
        );
        return (a + 0.05) / (b + 0.05);
      });
      expect(ratio, `title contrast on hover in ${colorScheme} mode`).toBeGreaterThan(4.5);
    }
  });
});

function matchSelectionFixtureScript(key: string): string {
  const chapters = [
    { id: "idx:0", order: 0, title: "Chapter One", body: "", kind: "body" },
  ];
  const mappingState = { pairs: [], parking_lot: [], op_id: 0 };
  return `;(() => {
    const key = ${JSON.stringify(key)};
    window.__pickerState__.chaptersByProject[key] = ${JSON.stringify(chapters)};
    window.__mappingState__.seed(key, ${JSON.stringify(mappingState)});
  })();`;
}

// A low-confidence, untouched pair so the grid renders its Confirm button —
// the control G1 is about (its own bg-surface, untouched by the colour remap).
function matchConfirmFixtureScript(key: string): string {
  const chapters = [
    { id: "idx:0", order: 0, title: "Chapter One", body: "", kind: "body" },
  ];
  const mappingState = {
    pairs: [
      {
        chapter_id: "idx:0",
        track_id: "t0",
        confidence: 0.5,
        original_confidence: 0.5,
        touched: false,
      },
    ],
    buckets: [
      {
        trackId: "t0",
        atomTitle: "Track One",
        atomDurationSec: 120,
        charsPerSec: 10,
        audioPath: null,
        window: [0, 120],
      },
    ],
    parking_lot: [],
    op_id: 0,
  };
  return `;(() => {
    const key = ${JSON.stringify(key)};
    window.__pickerState__.chaptersByProject[key] = ${JSON.stringify(chapters)};
    window.__mappingState__.seed(key, ${JSON.stringify(mappingState)});
  })();`;
}

test.describe("mapping grid selection", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("the selected chapter row's title stays legible on the fill", async ({
    page,
  }) => {
    const key = "match-selection-fixture";
    await page.addInitScript(matchSelectionFixtureScript(key));
    await page.goto(`/match/${key}`);
    await page.waitForLoadState("networkidle");
    const row = page.getByTestId("mapping-chapter-row").first();
    await row.click();
    await expect(row).toHaveAttribute("aria-selected", "true");
    await page.waitForTimeout(250); // let the row's transition-colors settle
    const contrastRatios = await row.evaluate((el) => {
      // Read colours back through a canvas: getComputedStyle can serialize
      // color-mix()/AccentColor output as oklab() with the mix's own alpha,
      // which a regex-based channel read would misparse. Compositing onto a
      // white 1x1 canvas resolves any function/alpha to the true rendered rgb.
      const composite = (c: string) => {
        const cvs = document.createElement("canvas");
        cvs.width = cvs.height = 1;
        const ctx = cvs.getContext("2d")!;
        ctx.fillStyle = "white";
        ctx.fillRect(0, 0, 1, 1);
        ctx.fillStyle = c;
        ctx.fillRect(0, 0, 1, 1);
        return Array.from(ctx.getImageData(0, 0, 1, 1).data);
      };
      const lum = (c: string) => {
        const [r, g, b] = composite(c);
        const f = (v: number) => {
          const s = v / 255;
          return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
        };
        return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
      };
      const contrast = (colorA: string, colorB: string) => {
        const [a, b] = [lum(colorA), lum(colorB)].sort((p, q) => q - p);
        return (a + 0.05) / (b + 0.05);
      };
      const fillColor = getComputedStyle(el).backgroundColor;
      const title = el.querySelector("span.flex-1.truncate")!;
      const chapterNumber = el.querySelector(
        '[data-testid="chapter-number"]',
      )!;
      return {
        title: contrast(getComputedStyle(title).color, fillColor),
        // The muted chapter-number remap (.text-fg-muted) is a separate,
        // lower-contrast mix — this is the regression probe for it.
        chapterNumber: contrast(
          getComputedStyle(chapterNumber).color,
          fillColor,
        ),
      };
    });
    expect(contrastRatios.title).toBeGreaterThan(4.5);
    expect(contrastRatios.chapterNumber).toBeGreaterThan(4.5);
  });

  test("Confirm keeps its own background on hover inside a selected row", async ({
    page,
  }) => {
    const key = "match-confirm-hover-fixture";
    await page.addInitScript(matchConfirmFixtureScript(key));
    for (const colorScheme of ["light", "dark"] as const) {
      await page.emulateMedia({ colorScheme });
      await page.goto(`/match/${key}`);
      await page.waitForLoadState("networkidle");
      const row = page.getByTestId("mapping-chapter-row").first();
      await row.click();
      await expect(row).toHaveAttribute("aria-selected", "true");
      const confirm = page.getByTestId("confirm-pair");
      await confirm.hover();
      await page.waitForTimeout(250); // let transition-colors settle
      const ratio = await confirm.evaluate((el) => {
        const composite = (c: string) => {
          const cvs = document.createElement("canvas");
          cvs.width = cvs.height = 1;
          const ctx = cvs.getContext("2d")!;
          ctx.fillStyle = "white";
          ctx.fillRect(0, 0, 1, 1);
          ctx.fillStyle = c;
          ctx.fillRect(0, 0, 1, 1);
          return Array.from(ctx.getImageData(0, 0, 1, 1).data);
        };
        const lum = (c: string) => {
          const [r, g, b] = composite(c);
          const f = (v: number) => {
            const s = v / 255;
            return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
          };
          return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
        };
        const cs = getComputedStyle(el);
        const [a, b] = [lum(cs.color), lum(cs.backgroundColor)].sort(
          (p, q) => q - p,
        );
        return (a + 0.05) / (b + 0.05);
      });
      expect(ratio, `Confirm hover contrast in ${colorScheme} mode`).toBeGreaterThan(4.5);
    }
  });
});

test.describe("button primitive", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("secondary buttons sit on the surface with a hairline shadow", async ({
    page,
  }) => {
    const key = "push-button-fixture";
    await page.addInitScript(pushButtonFixtureScript(key));
    await page.goto(`/match/${key}`);
    const button = page.getByRole("button", { name: "Reset detected range" });
    await expect(button).toBeVisible();
    const shadow = await button.evaluate(
      (el) => getComputedStyle(el).boxShadow,
    );
    expect(shadow).not.toBe("none");
  });

  test("no component hand-rolls an accent fill any more", async ({ page }) => {
    // /settings renders real Button primaries (Save, Save key) — a genuine
    // check that they all carry the `.btn` marker, not a vacuous one.
    await page.goto("/settings");
    await page.waitForLoadState("networkidle");
    const filled = page.locator("button.bg-accent:not(.btn)");
    await expect(filled).toHaveCount(0);
  });
});

test.describe("sheets", () => {
  test.beforeEach(async ({ page }, testInfo) => {
    await page.addInitScript(tauriStubInitScriptFor(testInfo.workerIndex));
  });

  test("a modal is attached to the top edge, not centred", async ({ page }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const probe = await page.evaluate(() => {
      const el = document.createElement("dialog");
      el.innerHTML = '<div class="sheet-card">probe</div>';
      document.body.append(el);
      el.showModal();
      const card = el.querySelector(".sheet-card")!;
      return {
        box: el.getBoundingClientRect(),
        cardBg: getComputedStyle(card).backgroundColor,
      };
    });
    expect(probe.box.y).toBeLessThan(24);
    // Position alone would pass even if .sheet-card did nothing — the dialog
    // rule sets top:0 on its own. Assert the card fill actually applied too.
    expect(probe.cardBg).not.toBe("rgba(0, 0, 0, 0)");
  });

  test("the sheet animates normally, but reduced motion suppresses it", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    const measure = () =>
      page.evaluate(() => {
        const el = document.createElement("dialog");
        document.body.append(el);
        el.showModal();
        const value = getComputedStyle(el).animationDuration;
        el.close();
        el.remove();
        return value;
      });

    // 0s/0ms is also the initial value, so on its own the reduced-motion
    // assertion below can't tell "suppressed" from "never animated" —
    // this confirms the animation is really there when motion is allowed.
    expect(await measure()).toBe("0.28s");

    await page.emulateMedia({ reducedMotion: "reduce" });
    expect(["0s", "0ms"]).toContain(await measure());
  });

  test("reduced motion collapses transitions but exempts loading spinners", async ({
    page,
  }) => {
    await page.goto("/library");
    await page.waitForLoadState("networkidle");
    await page.emulateMedia({ reducedMotion: "reduce" });
    const durations = await page.evaluate(() => {
      const spin = document.createElement("div");
      spin.className = "animate-spin";
      document.body.append(spin);
      const plain = document.createElement("div");
      document.body.append(plain);
      const out = {
        spin: getComputedStyle(spin).animationDuration,
        plain: getComputedStyle(plain).animationDuration,
      };
      spin.remove();
      plain.remove();
      return out;
    });
    expect(durations.plain).toBe("1e-05s"); // 0.01ms, as Chromium serializes it
    expect(durations.spin).not.toBe("1e-05s");
  });
});
