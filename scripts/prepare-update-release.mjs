import fs from 'node:fs';
const key = process.env.ACE_UPDATER_PUBLIC_KEY?.trim();
if (!key || !Buffer.from(key, 'base64').toString('utf8').includes('untrusted comment:'))
  throw new Error('ACE_UPDATER_PUBLIC_KEY에 Tauri 공개키를 설정해야 합니다.');
const checkOnly = process.argv.includes('--check-only');
if (!checkOnly && !process.env.TAURI_SIGNING_PRIVATE_KEY)
  throw new Error('CI secret TAURI_SIGNING_PRIVATE_KEY가 필요합니다.');
fs.writeFileSync(
  'src-tauri/tauri.update.generated.json',
  JSON.stringify(
    {
      bundle: { createUpdaterArtifacts: !checkOnly },
      plugins: {
        updater: {
          pubkey: key,
          endpoints: [
            'https://github.com/AI-AGENT-ACE/ace-desktop/releases/latest/download/latest.json',
          ],
          windows: { installMode: 'passive' },
        },
      },
    },
    null,
    2,
  ),
);
