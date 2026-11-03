import { spawnSync } from 'node:child_process';
const result = spawnSync(
  'cargo',
  [
    'run',
    '--quiet',
    '--manifest-path',
    'src-tauri/Cargo.toml',
    '--example',
    'validate_public_wake',
  ],
  { stdio: 'inherit' },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
