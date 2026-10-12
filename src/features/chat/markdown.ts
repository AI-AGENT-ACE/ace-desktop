export function normalizeAssistantMarkdown(content: string): string {
  // Markdown hard breaks keep headings and lists parseable without enabling raw HTML.
  return content.replace(/<br\s*\/?\s*>/gi, '  \n');
}
