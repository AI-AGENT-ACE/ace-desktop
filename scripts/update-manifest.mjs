import fs from 'node:fs';
import path from 'node:path';
const directory = 'src-tauri/target/release/bundle/nsis';
const files = fs.readdirSync(directory).filter((name) => name.endsWith('.exe'));
if (files.length !== 1) throw new Error('업데이트용 NSIS 설치 파일이 정확히 하나 필요합니다.');
const installer = files[0];
const signature = fs.readFileSync(path.join(directory, `${installer}.sig`), 'utf8').trim();
if (!signature || fs.statSync(path.join(directory, installer)).size === 0)
  throw new Error('설치 파일 또는 서명이 없습니다.');
const version = JSON.parse(fs.readFileSync('package.json', 'utf8')).version;
const tag = `v${version}`;
const notes = fs.readFileSync('docs/RELEASE_NOTES.md', 'utf8');
fs.writeFileSync(
  path.join(directory, 'latest.json'),
  JSON.stringify(
    {
      version,
      notes,
      pub_date: new Date().toISOString(),
      platforms: {
        'windows-x86_64': {
          signature,
          url: `https://github.com/AI-AGENT-ACE/ace-desktop/releases/download/${tag}/${encodeURIComponent(installer)}`,
        },
      },
    },
    null,
    2,
  ),
);
