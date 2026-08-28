// The Library's filters live in its query string, and both the sidebar links
// and the page's own navigations have to spell them the same way: language
// first, then q, empty values omitted.
export function libraryUrl(language: string, query = ""): string {
  const params = new URLSearchParams();
  if (language) params.set("language", language);
  if (query) params.set("q", query);
  const suffix = params.toString();
  return suffix ? `/library?${suffix}` : "/library";
}
