import { mkdir, rename } from 'node:fs/promises';
import { createReadStream, createWriteStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { pipeline } from 'node:stream/promises';
import { Readable } from 'node:stream';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const directory = fileURLToPath(new URL('../.local/whisper/', import.meta.url));
await mkdir(directory, { recursive: true });
async function digest(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}
async function download(name, url, expected) {
  const destination = path.join(directory, name);
  if (
    await digest(destination)
      .then((value) => value === expected)
      .catch(() => false)
  )
    return destination;
  console.log(`Downloading ${name}...`);
  const response = await fetch(url);
  if (!response.ok || !response.body) throw new Error(`Download failed: ${response.status}`);
  const temporary = destination + '.part';
  await pipeline(Readable.fromWeb(response.body), createWriteStream(temporary));
  if ((await digest(temporary)) !== expected) throw new Error(`Checksum mismatch: ${name}`);
  await rename(temporary, destination);
  console.log(`Verified ${name}`);
  return destination;
}
const archive = await download(
  'whisper-bin-x64.zip',
  'https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.2/whisper-bin-x64.zip',
  '49dcc16de826f20bd53d44f947a1ae49dfa81f86cad67a64d80820cb192d674a',
);
const runtime = path.join(directory, 'runtime');
await mkdir(runtime, { recursive: true });
const extracted = spawnSync('tar.exe', ['-xf', archive, '-C', runtime], {
  windowsHide: true,
  stdio: 'inherit',
});
if (extracted.error || extracted.status !== 0) throw new Error('Could not extract Whisper runtime');
await download(
  'ggml-small-q5_1.bin',
  'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin',
  'ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb',
);
console.log('Local speech recognition runtime and Korean-capable model are ready.');
