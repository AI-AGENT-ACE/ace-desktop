import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import ts from 'typescript';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

const source = await readFile(new URL('../src/features/chat/markdown.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext },
});
const { normalizeAssistantMarkdown } = await import(
  `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`
);
const render = (content) =>
  renderToStaticMarkup(
    React.createElement(
      ReactMarkdown,
      { remarkPlugins: [remarkGfm] },
      normalizeAssistantMarkdown(content),
    ),
  );

test('renders mixed AI line breaks, headings, emphasis and nested lists', () => {
  const html = render(
    '**서울 여행**<br><br>### 1. **숙박비**<br>- **호텔**: 20만원<br>  - 예시: 명동<br><br>첫 줄<BR />둘째 줄<br/>셋째 줄',
  );
  assert.match(html, /<h3>1\. <strong>숙박비<\/strong><\/h3>/);
  assert.match(html, /<strong>서울 여행<\/strong>/);
  assert.equal((html.match(/<ul>/g) || []).length, 2);
  assert.match(html, /첫 줄<br\/>\n둘째 줄<br\/>\n셋째 줄/);
  assert.doesNotMatch(html, /&lt;br/i);
});
test('preserves standard Markdown tables, ordered lists and safe HTML handling', () => {
  const html = render(
    '# 제목\n\n1. 첫째\n2. 둘째\n\n| 항목 | 비용 |\n| --- | --- |\n| 숙박 | 20만원 |\n\n<script>alert(1)</script>',
  );
  assert.match(html, /<h1>제목<\/h1>/);
  assert.match(html, /<ol>/);
  assert.match(html, /<table>/);
  assert.doesNotMatch(html, /<script>/);
});
