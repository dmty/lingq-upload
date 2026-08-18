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

// Page-context code belongs in functions Playwright serializes, not in template
// literals: a string is invisible to the compiler, the formatter and the
// editor, so fixture drift only shows up as a failed screen assertion.
test("no spec builds page-context JavaScript as a string", () => {
  const offenders = specFiles("e2e")
    .filter((p) => !p.endsWith("no-stringified-scripts.spec.ts"))
    .filter((p) => {
      const source = readFileSync(p, "utf8");
      return (
        /addInitScript\(\s*`/.test(source) ||
        /window\.__\w+__\s*=\s*\$\{/.test(source)
      );
    });
  expect(offenders).toEqual([]);
});
