import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";

function specFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    return statSync(p).isDirectory()
      ? specFiles(p)
      : p.endsWith(".ts")
        ? [p]
        : [];
  });
}

// Comments legitimately reference `window.__foo__` in backticks (see
// tauri-stub.ts's own doc comment) — strip them before scanning so those
// don't read as page-context code.
function stripComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");
}

// Extracted individually (rather than one regex over the whole file) so a
// gap between two unrelated literals — e.g. `` `/a/${x}` `` ... real code
// mentioning window.__ ... `` `/b/${y}` `` — can never be misread as the
// inside of a literal.
function templateLiterals(source: string): string[] {
  return source.match(/`[^`]*`/g) ?? [];
}

// Page-context code belongs in functions Playwright serializes, not in
// template literals: a string is invisible to the compiler, the formatter
// and the editor, so fixture drift only shows up as a failed screen
// assertion. This also has to catch indirection — a helper that builds a
// template literal and hands it to addInitScript by identifier is the exact
// shape every fixture in this suite used before the refactor — so any
// template literal that mentions window.__ is treated as an offender
// regardless of how it reaches the page, alongside the direct call forms.
test("no spec builds page-context JavaScript as a string", () => {
  const offenders = specFiles("e2e")
    .filter((p) => !p.endsWith("no-stringified-scripts.spec.ts"))
    .filter((p) => {
      const source = stripComments(readFileSync(p, "utf8"));
      return (
        /addInitScript\(\s*`/.test(source) ||
        /\.evaluate\(\s*`/.test(source) ||
        templateLiterals(source).some((literal) =>
          literal.includes("window.__"),
        )
      );
    });
  expect(offenders).toEqual([]);
});
