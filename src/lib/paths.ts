export function extOf(path: string): string {
  const dot = path.lastIndexOf(".");
  return dot >= 0 ? path.slice(dot + 1).toLowerCase() : "";
}

export function basename(path: string): string {
  if (!path) return "";
  const sep = path.lastIndexOf("/") >= 0 ? "/" : "\\";
  return path.split(sep).pop() ?? path;
}

export function filenameStem(path: string): string {
  const base = basename(path);
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(0, dot) : base;
}
