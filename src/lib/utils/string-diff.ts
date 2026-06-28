export type DiffKind = "equal" | "add" | "del";
export type DiffSegment = { kind: DiffKind; text: string };

export type DiffResult = {
  a: DiffSegment[];
  b: DiffSegment[];
};

// LCS-based diff. Strings are short (titles, filenames — typically < 200
// chars), so the O(n*m) table is cheap and the result is easier to reason
// about than Myers for runs of equal characters.
export function diffStrings(a: string, b: string): DiffResult {
  if (a === b) {
    if (a.length === 0) return { a: [], b: [] };
    return {
      a: [{ kind: "equal", text: a }],
      b: [{ kind: "equal", text: b }],
    };
  }
  if (a.length === 0) {
    return { a: [], b: [{ kind: "add", text: b }] };
  }
  if (b.length === 0) {
    return { a: [{ kind: "del", text: a }], b: [] };
  }

  const n = a.length;
  const m = b.length;
  // dp[i][j] = LCS length of a[..i] vs b[..j]. Flat row-major Uint16 keeps
  // allocation tight for the typical short-string case; > 65535 LCS is
  // impossible for strings this size.
  const dp = new Uint16Array((n + 1) * (m + 1));
  const stride = m + 1;
  for (let i = 1; i <= n; i++) {
    const ai = a.charCodeAt(i - 1);
    const rowBase = i * stride;
    const prevBase = (i - 1) * stride;
    for (let j = 1; j <= m; j++) {
      if (ai === b.charCodeAt(j - 1)) {
        dp[rowBase + j] = dp[prevBase + j - 1] + 1;
      } else {
        const up = dp[prevBase + j];
        const left = dp[rowBase + j - 1];
        dp[rowBase + j] = up >= left ? up : left;
      }
    }
  }

  // Backtrack from (n, m) to (0, 0). Each cell yields one of equal / del / add.
  type Step = { kind: DiffKind; ch: string };
  const steps: Step[] = [];
  let i = n;
  let j = m;
  while (i > 0 && j > 0) {
    if (a.charCodeAt(i - 1) === b.charCodeAt(j - 1)) {
      steps.push({ kind: "equal", ch: a[i - 1] });
      i--;
      j--;
    } else if (dp[(i - 1) * stride + j] >= dp[i * stride + j - 1]) {
      steps.push({ kind: "del", ch: a[i - 1] });
      i--;
    } else {
      steps.push({ kind: "add", ch: b[j - 1] });
      j--;
    }
  }
  while (i > 0) {
    steps.push({ kind: "del", ch: a[i - 1] });
    i--;
  }
  while (j > 0) {
    steps.push({ kind: "add", ch: b[j - 1] });
    j--;
  }
  steps.reverse();

  // Coalesce neighbours of the same kind, then split into a/b ribbons.
  const aOut: DiffSegment[] = [];
  const bOut: DiffSegment[] = [];
  function push(target: DiffSegment[], kind: DiffKind, ch: string) {
    const last = target[target.length - 1];
    if (last && last.kind === kind) last.text += ch;
    else target.push({ kind, text: ch });
  }
  for (const s of steps) {
    if (s.kind === "equal") {
      push(aOut, "equal", s.ch);
      push(bOut, "equal", s.ch);
    } else if (s.kind === "del") {
      push(aOut, "del", s.ch);
    } else {
      push(bOut, "add", s.ch);
    }
  }
  return { a: aOut, b: bOut };
}
