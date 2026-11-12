export function lingqCollectionUrl(lang: string, id: number): string {
  return `https://www.lingq.com/${lang}/learn/${lang}/web/library/course/${id}`;
}

export function lingqLessonUrl(lang: string, id: number): string {
  return `https://www.lingq.com/${lang}/learn/${lang}/web/reader/${id}`;
}
