// Exercise the real component's transitions and IPC calls with a mocked native boundary.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import ts from 'typescript';

async function setup() {
  const state = [];
  let cursor = 0;
  const calls = [];
  const results = { detected: true, accepted: true, failStart: false };
  const context = vm.createContext({ console });
  const exports = {
    react: {
      useEffect: () => {},
      useState: initial => {
        const index = cursor++;
        if (!(index in state)) state[index] = initial;
        return [state[index], value => { state[index] = typeof value === 'function' ? value(state[index]) : value; }];
      },
    },
    'react/jsx-runtime': {
      jsx: (type, props) => ({ type, props }),
      jsxs: (type, props) => ({ type, props }),
      Fragment: 'fragment',
    },
    'lucide-react': Object.fromEntries(['Check', 'LoaderCircle', 'Mic', 'ShieldCheck'].map(name => [name, name])),
    '../../components/Modal': { Modal: 'modal' },
    '@tauri-apps/api/core': {
      invoke: async name => {
        calls.push(name);
        if (name.startsWith('start_wake_word') && results.failStart) throw Error('device error');
        if (name === 'prepare_wake_word_setup') return { completedSamples: 0, totalSamples: 5 };
        if (name === 'finish_wake_word_sample') return { accepted: results.accepted };
        if (name === 'test_wake_word_reference') return results.detected;
      },
    },
  };
  const source = await readFile(new URL('../src/features/voice/WakeWordSetupModal.tsx', import.meta.url), 'utf8');
  const module = new vm.SourceTextModule(ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX },
  }).outputText, { context, initializeImportMeta: meta => { meta.env = { DEV: false }; } });
  await module.link(name => new vm.SyntheticModule(Object.keys(exports[name]), function() {
    for (const [key, value] of Object.entries(exports[name])) this.setExport(key, value);
  }, { context }));
  await module.evaluate();
  const render = () => { cursor = 0; return module.namespace.WakeWordSetupModal({ onClose() {}, onCompleted() {} }); };
  const text = node => Array.isArray(node) ? node.map(text).join('') : typeof node === 'object' && node ? text(node.props?.children) : typeof node === 'string' || typeof node === 'number' ? String(node) : '';
  const nodes = node => Array.isArray(node) ? node.flatMap(nodes) : typeof node === 'object' && node ? [node, ...nodes(node.props?.children)] : [];
  const click = async label => {
    const button = nodes(render()).find(node => node.type === 'button' && text(node) === label);
    assert.ok(button, `button exists: ${label}`);
    assert.ok(!button.props.disabled, `button enabled: ${label}`);
    button.props.onClick();
    for (let i = 0; i < 20; i++) await Promise.resolve();
  };
  return { calls, results, click, text: () => text(render()) };
}

async function enroll(ui) {
  await ui.click('음성 호출 설정하기');
  for (let i = 0; i < 5; i++) {
    await ui.click('녹음 시작');
    await ui.click('녹음 끝내기');
  }
}
async function check(ui) {
  await ui.click('테스트 시작');
  await ui.click('테스트 끝내기');
}

test('registration uses native capture; activation requires two consecutive live tests', async () => {
  const ui = await setup();
  await enroll(ui);
  await check(ui);
  assert.match(ui.text(), /1\/2/);
  assert.ok(!ui.calls.includes('complete_wake_word_setup'));
  ui.results.detected = false;
  await check(ui);
  assert.match(ui.text(), /0\/2/);
  ui.results.detected = true;
  await check(ui);
  assert.ok(!ui.calls.includes('complete_wake_word_setup'));
  await check(ui);
  assert.equal(ui.calls.filter(c => c === 'complete_wake_word_setup').length, 1);
  assert.match(ui.text(), /등록 테스트를 통과/);
  assert.ok(!ui.calls.includes('append_wake_word_sample'));
});

test('bad audio clears progress; native startup failure remains retryable', async () => {
  const ui = await setup();
  await enroll(ui);
  await check(ui);
  ui.results.accepted = false;
  await check(ui);
  assert.match(ui.text(), /0\/2/);
  ui.results.accepted = true;
  ui.results.failStart = true;
  await ui.click('테스트 시작');
  assert.match(ui.text(), /녹음을 시작하지 못했습니다/);
  ui.results.failStart = false;
  await check(ui);
  await check(ui);
  assert.equal(ui.calls.filter(c => c === 'complete_wake_word_setup').length, 1);
});
