import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";

function svelteFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    return statSync(p).isDirectory()
      ? svelteFiles(p)
      : p.endsWith(".svelte")
        ? [p]
        : [];
  });
}

test("no component hand-rolls a button the primitive already provides", () => {
  const offenders = svelteFiles("src")
    .filter((p) => !p.endsWith("Button.svelte"))
    .filter((p) => /bg-accent\s+px-|bg-accent\s+hover:bg-accent-hover/.test(readFileSync(p, "utf8")));
  expect(offenders).toEqual([]);
});
