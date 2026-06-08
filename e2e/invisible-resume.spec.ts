import { expect, test, type Page } from "@playwright/test";

import { tauriStubInitScript } from "./setup/tauri-stub";

// AD-025: the user-experience contract is that the rendered DOM never
// surfaces recovery-event language. The state machine + cancellation +
// atomic writes exist to make recovery silent. Each forbidden word below
// breaches that contract:
//   crash       — implies a fault the user must react to
//   recover     — implies a prior failure they need to know about
//   interrupted — implies their flow was disturbed
//   restored    — implies something was lost and rebuilt
//   resume      — implies recovery from an interruption (allowed only as
//                 a user-driven action verb on a button)
const FORBIDDEN_ANYWHERE = ["crash", "recover", "interrupted", "restored"];
const FORBIDDEN_OUTSIDE_BUTTON = "resume";

// "Resume" is a legitimate button label when a project has prior receipts
// (Start vs Resume). It is a user-driven action verb, not a system-emitted
// recovery notification. Filter button text out before grepping so the
// button-as-action case stays legal while the system-message case stays
// forbidden.
async function nonButtonText(page: Page): Promise<string> {
  return await page.evaluate(() => {
    const clone = document.body.cloneNode(true) as HTMLElement;
    clone
      .querySelectorAll("button, [role='button']")
      .forEach((el) => el.remove());
    return (clone.innerText || "").toLowerCase();
  });
}

// Screen-reader-only spans, aria-live regions, and decorative imagery can
// carry forbidden words invisible to innerText. Walk every aria-label,
// title, alt, and aria-describedby payload too — AT users count.
async function ariaAttributeText(page: Page): Promise<string> {
  return await page.evaluate(() => {
    const attrs = ["aria-label", "title", "alt", "aria-describedby"];
    const out: string[] = [];
    for (const el of Array.from(document.querySelectorAll("*"))) {
      for (const a of attrs) {
        const v = el.getAttribute(a);
        if (v) out.push(v);
      }
    }
    return out.join(" \n ").toLowerCase();
  });
}

test.describe("invisible resilience (AD-025)", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(tauriStubInitScript);
  });

  for (const route of [
    "/",
    "/library",
    "/add",
    "/settings",
    "/match/some-project",
    "/run/some-project",
  ]) {
    test(`forbidden recovery words absent on ${route}`, async ({ page }) => {
      await page.goto(route);
      await page.waitForLoadState("networkidle");

      const visible = await nonButtonText(page);
      const aria = await ariaAttributeText(page);

      for (const word of FORBIDDEN_ANYWHERE) {
        expect(
          visible,
          `forbidden word "${word}" found in visible DOM on ${route}`,
        ).not.toContain(word);
        expect(
          aria,
          `forbidden word "${word}" found in aria/title/alt on ${route}`,
        ).not.toContain(word);
      }
      expect(
        visible,
        `"${FORBIDDEN_OUTSIDE_BUTTON}" leaked outside button on ${route}`,
      ).not.toContain(FORBIDDEN_OUTSIDE_BUTTON);
      expect(
        aria,
        `"${FORBIDDEN_OUTSIDE_BUTTON}" leaked into aria/title/alt on ${route}`,
      ).not.toContain(FORBIDDEN_OUTSIDE_BUTTON);
    });
  }
});
