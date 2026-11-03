import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

function harness(enabled = true, pending = false) {
  const calls = [], audio = [];
  let resolve;
  const exports = {};
  const code = ts.transpileModule(readFileSync('src/api/speech.api.ts', 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
  vm.runInNewContext(code, { exports, AbortController, require: name => name === './settings.api'
    ? { settingsApi: { get: async () => ({ ttsEnabled: enabled }) } }
    : { apiClient: { post: async (...args) => { calls.push(args); return pending ? new Promise(r => { resolve = r; }) : { data: {} }; } } },
    URL: { createObjectURL: () => 'blob:test', revokeObjectURL: () => {} },
    Audio: class { constructor() { audio.push(this); } async play() { this.played = true; } pause() { this.paused = true; } },
  });
  return { ...exports, calls, audio, resolve: () => resolve({ data: {} }) };
}
test('disabled speech does not request audio', async () => {
  const h = harness(false); await h.speakAssistant('hello'); assert.equal(h.calls.length, 0);
});
test('speech strips formatting and stops playback', async () => {
  const h = harness(); await h.speakAssistant('<strong>안녕</strong><br>**테스트**');
  assert.equal(h.calls[0][1].text, '안녕\n테스트'); assert.equal(h.audio[0].played, true);
  h.stopSpeech(); assert.equal(h.audio[0].paused, true);
});
test('stopped pending audio never starts later', async () => {
  const h = harness(true, true); const task = h.speakAssistant('hello');
  await new Promise(setImmediate); h.stopSpeech(); h.resolve(); await task;
  assert.equal(h.calls[0][2].signal.aborted, true); assert.equal(h.audio.length, 0);
});
