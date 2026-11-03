export function normalizeAssistantMarkdown(content: string): string {
  // Markdown hard breaks keep headings and lists parseable without enabling raw HTML.
  // Preserve literal examples in fenced/inline code; never enable arbitrary HTML.
  return content
    .split(/(```[\s\S]*?```|~~~[\s\S]*?~~~|`[^`\n]*`)/g)
    .map((part, index) =>
      index % 2
        ? part
        : part
            .replace(/<br\s*\/?\s*>/gi, '  \n')
            .replace(
              /<(strong|b)\s*>([\s\S]*?)<\/\1\s*>/gi,
              (_, _tag, text: string) => `**${text.trim()}**`,
            )
            .replace(
              /<(em|i)\s*>([\s\S]*?)<\/\1\s*>/gi,
              (_, _tag, text: string) => `*${text.trim()}*`,
            ),
    )
    .join('');
}
