import fs from 'node:fs';
import path from 'node:path';
function inspect(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      inspect(file);
      continue;
    }
    if (!/\.(js|html|map)$/.test(entry.name)) continue;
    const content = fs.readFileSync(file, 'utf8');
    if (
      ['native-dev-panel', 'NativeTestPanel', 'Native 테스트 · 개발 전용'].some((marker) =>
        content.includes(marker),
      )
    )
      throw new Error('Development Native panel leaked into production output.');
  }
}
inspect('dist');
console.log('Production output excludes the Native development panel and its entry point.');
