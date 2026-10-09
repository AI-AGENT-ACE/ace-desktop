import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import ts from 'typescript';

async function settings(overrides = {}) {
  const calls = [];
  const context = vm.createContext({ console });
  const exports = {
    '../../../package.json': { default: { version: '0.1.0' } },
    react: {
      useEffect() {},
      useRef: (value) => ({ current: value }),
      useState: (value) => [value, () => {}],
    },
    'react/jsx-runtime': {
      Fragment: 'fragment',
      jsx: (type, props) => ({ type, props }),
      jsxs: (type, props) => ({ type, props }),
    },
    'lucide-react': { Trash2: 'trash' },
    '../../api/settings.api': { settingsApi: {} },
    '../../api/client': { apiErrorMessage: String, isCancelled: () => false },
    '../../components/Modal': { Modal: 'modal' },
    './PermissionSettings': { PermissionSettings: 'permissions' },
    '../voice/WakeDiagnosticPanel': { WakeDiagnosticPanel: 'diagnostics' },
    '../voice/WakeSensitivitySettings': { WakeSensitivitySettings: 'sensitivity' },
    '../voice/WakeChimeSettings': { WakeChimeSettings: 'chime' },
  };
  const source = await readFile(
    new URL('../src/features/settings/SettingsModal.tsx', import.meta.url),
    'utf8',
  );
  const module = new vm.SourceTextModule(
    ts.transpileModule(source, {
      compilerOptions: {
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.ESNext,
        jsx: ts.JsxEmit.ReactJSX,
      },
    }).outputText,
    {
      context,
      initializeImportMeta: (meta) => {
        meta.env = { DEV: false };
      },
    },
  );
  await module.link(
    (name) =>
      new vm.SyntheticModule(
        Object.keys(exports[name]),
        function () {
          for (const [key, value] of Object.entries(exports[name])) this.setExport(key, value);
        },
        { context },
      ),
  );
  await module.evaluate();
  const tree = module.namespace.SettingsModal({
    theme: 'light',
    wake: false,
    wakeState: 'disabled',
    wakeSetupCompleted: false,
    wakeReferenceExists: true,
    wakeDefaultAvailable: true,
    wakeModelSource: 'default',
    onWake: (enabled) => calls.push(['wake', enabled]),
    onWakeSetup: () => calls.push(['setup']),
    onWakeModel: async (source) => calls.push(['model', source]),
    ...overrides,
  });
  const nodes = (node) =>
    Array.isArray(node)
      ? node.flatMap(nodes)
      : node && typeof node === 'object'
        ? [node, ...nodes(node.props?.children)]
        : [];
  const text = (node) =>
    Array.isArray(node)
      ? node.map(text).join('')
      : node && typeof node === 'object'
        ? text(node.props?.children)
        : typeof node === 'string'
          ? node
          : '';
  return { calls, nodes: nodes(tree), text };
}

test('new user enables wake directly without enrollment; calibration remains optional', async () => {
  const ui = await settings();
  const toggle = ui.nodes.find((node) => node.props?.role === 'switch');
  assert.equal(toggle.props.disabled, false);
  toggle.props.onClick();
  assert.deepEqual(ui.calls, [['wake', true]]);
  const calibration = ui.nodes.find(
    (node) => node.type === 'button' && ui.text(node) === '내 목소리로 ACE 호출어 녹음',
  );
  calibration.props.onClick();
  assert.deepEqual(ui.calls, [['wake', true], ['setup']]);
});

test('existing enrollment remains optional without obsolete model switching or diagnostic controls', async () => {
  const ui = await settings({ wakeSetupCompleted: true, wakeModelSource: 'personal' });
  assert.equal(
    ui.nodes.some((node) => node.type === 'button' && ui.text(node) === '기본 호출로 전환'),
    false,
  );
  assert.equal(
    ui.nodes.some((node) => node.type === 'diagnostics'),
    false,
  );
  assert.ok(
    ui.nodes.some(
      (node) =>
        node.props?.className === 'settings-footnote' && ui.text(node).includes('ACE 0.1.0'),
    ),
  );
  assert.deepEqual(ui.calls, []);
});

test('missing model is reported rather than silently opening enrollment', async () => {
  const ui = await settings({
    wakeReferenceExists: false,
    wakeDefaultAvailable: false,
    wakeModelSource: null,
  });
  assert.equal(ui.nodes.find((node) => node.props?.role === 'switch').props.disabled, true);
  assert.deepEqual(ui.calls, []);
});
