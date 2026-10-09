import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import ts from 'typescript';

async function harness() {
  const hooks = [];
  let cursor = 0;
  let now = 0;
  let timerId = 0;
  const timers = new Map(),
    listeners = new Map(),
    calls = [],
    processors = [],
    effects = [];
  let result = { transcript: '포트폴리오 열어줘', portfolioRequested: true };
  const clock = {
    setTimeout: (fn, delay = 0) => {
      timers.set(++timerId, { at: now + delay, fn });
      return timerId;
    },
    clearTimeout: (id) => timers.delete(id),
  };
  const flush = async () => {
    for (let i = 0; i < 80; i++) await Promise.resolve();
  };
  const advance = async (ms) => {
    now += ms;
    for (const [id, timer] of [...timers])
      if (timer.at <= now) {
        timers.delete(id);
        timer.fn();
        await flush();
      }
  };
  const track = { stop() {} };
  class AudioContext {
    sampleRate = 48000;
    state = 'running';
    async resume() {}
    async close() {
      this.state = 'closed';
    }
    createMediaStreamSource() {
      return { connect() {}, disconnect() {} };
    }
    createGain() {
      return { gain: { value: 0 }, connect() {}, disconnect() {} };
    }
    createScriptProcessor() {
      const p = { onaudioprocess: null, connect() {}, disconnect() {} };
      processors.push(p);
      return p;
    }
  }
  const context = vm.createContext({
    console,
    ...clock,
    crypto: globalThis.crypto,
    AudioContext,
    localStorage: { getItem: (key) => (key === 'ace-wake-chime' ? 'off' : null) },
    document: { documentElement: { dataset: {} } },
    navigator: { mediaDevices: { getUserMedia: async () => ({ getTracks: () => [track] }) } },
  });
  const synthetic = (name, values) =>
    new vm.SyntheticModule(
      Object.keys(values),
      function () {
        for (const [key, value] of Object.entries(values)) this.setExport(key, value);
      },
      { context, identifier: name },
    );
  const mocks = {
    react: {
      useState: (initial) => {
        const index = cursor++;
        if (!(index in hooks)) hooks[index] = typeof initial === 'function' ? initial() : initial;
        return [
          hooks[index],
          (value) => {
            hooks[index] = typeof value === 'function' ? value(hooks[index]) : value;
          },
        ];
      },
      useRef: (initial) => {
        const index = cursor++;
        return (hooks[index] ??= { current: initial });
      },
      useEffect: (callback) => {
        const index = cursor++;
        if (!hooks[index]) {
          hooks[index] = true;
          effects.push(callback);
        }
      },
    },
    'react/jsx-runtime': {
      jsx: (type, props, key) => ({ type, props, key }),
      jsxs: (type, props, key) => ({ type, props, key }),
    },
    'motion/react': { AnimatePresence: 'presence', useMotionValue: () => ({ set() {} }) },
    './VoiceOverlay': { VoiceOverlay: 'overlay' },
    '@tauri-apps/api/event': {
      listen: async (name, fn) => {
        listeners.set(name, fn);
        return () => listeners.delete(name);
      },
    },
    '@tauri-apps/api/core': {
      invoke: async (name, args) => {
        calls.push({ name, args });
        if (name === 'stop_voice_recording') return { recordingId: `recording-${calls.length}` };
        if (name === 'transcribe_local_portfolio')
          return typeof result === 'function' ? result() : result;
        if (name === 'hide_voice_overlay') listeners.get('ace-voice-reset')?.({ payload: null });
      },
    },
    '../../api/client': { apiBaseUrl: 'http://example.invalid', apiErrorMessage: (e) => String(e) },
    '../../api/voice-session': { requestVoiceAccessToken: async () => 'test-only' },
    '../../styles/globals.css': {},
  };
  const cache = new Map();
  async function load(name) {
    if (cache.has(name)) return cache.get(name);
    if (mocks[name]) {
      const mod = synthetic(name, mocks[name]);
      cache.set(name, mod);
      return mod;
    }
    const file = name === 'window' ? 'VoiceWindow.tsx' : name.replace('./', '') + '.ts';
    const source = await readFile(
      new URL('../src/features/voice/' + file, import.meta.url),
      'utf8',
    );
    const mod = new vm.SourceTextModule(
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
          meta.env = { DEV: false, VITE_VOICE_MODE: 'local-portfolio' };
        },
      },
    );
    cache.set(name, mod);
    await mod.link(load);
    return mod;
  }
  const module = await load('window');
  await module.evaluate();
  const render = () => {
    cursor = 0;
    return module.namespace.VoiceWindow().props.children.props.children;
  };
  render();
  for (const effect of effects) effect();
  await flush();
  const event = async (name, payload) => {
    listeners.get(name)?.({ payload });
    await flush();
  };
  const feed = async (rms, frames) => {
    for (let i = 0; i < frames; i++) {
      processors.at(-1)?.onaudioprocess?.({
        inputBuffer: {
          sampleRate: 48000,
          getChannelData: () => new Float32Array(4800).fill(rms),
        },
      });
      await flush();
    }
  };
  return {
    calls,
    render,
    event,
    feed,
    advance,
    flush,
    recognize: (value) => {
      result = value;
    },
    policy: cache.get('./voice-session-policy').namespace,
    activate: () => event('ace-voice-activate', null),
    say: async () => {
      await feed(0.08, 5);
      await feed(0, 15);
    },
    close: async () => {
      render().props.onClose();
      await flush();
    },
  };
}

test('actual tool completion starts the 3 second close timer; old results cannot close a new session', async () => {
  const h = await harness();
  await h.activate();
  await h.say();
  const request = h.calls.find((c) => c.name === 'submit_voice_tool_call').args.requestId;
  await h.advance(3000);
  assert.ok(h.render(), 'submission is not success');
  await h.event('ace-voice-tool-result', {
    requestId: request,
    status: 'SUCCESS',
    message: '완료',
    choices: [],
  });
  await h.advance(2999);
  assert.ok(h.render());
  await h.advance(1);
  assert.equal(h.render(), false);
  await h.activate();
  const key = h.render().key;
  await h.event('ace-voice-tool-result', {
    requestId: request,
    status: 'SUCCESS',
    message: 'old',
    choices: [],
  });
  await h.advance(3000);
  assert.ok(h.render());
  assert.equal(h.render().key, key);
});

test('failed recognition retries for 5 seconds; speech beginning near the deadline continues and can succeed', async () => {
  const h = await harness();
  h.recognize({ transcript: '포트폴리오예요', portfolioRequested: false });
  await h.activate();
  const key = h.render().key;
  await h.say();
  assert.equal(h.render().key, key, 'retry does not remount the orb');
  await h.feed(0, 49);
  await h.feed(0.08, 2);
  assert.ok(h.render());
  h.recognize({ transcript: '포트폴리오 열어줘', portfolioRequested: true });
  await h.feed(0.08, 4);
  await h.feed(0, 15);
  assert.equal(h.calls.filter((c) => c.name === 'submit_voice_tool_call').length, 1);
});

test('silent retry closes; repeated failures remain retryable', async () => {
  const h = await harness();
  h.recognize({ transcript: '다른 말', portfolioRequested: false });
  await h.activate();
  await h.say();
  await h.say();
  assert.equal(h.render().props.transcript, '제대로 인식하지 못했습니다. 다시 말씀해주세요.');
  assert.equal(h.calls.filter((c) => c.name === 'transcribe_local_portfolio').length, 2);
  await h.feed(0, 49);
  assert.ok(h.render());
  await h.feed(0, 1);
  assert.equal(h.render(), false);
});

test('main chat opens a confirmation Orb directly and a click submits only once', async () => {
  const h = await harness();
  await h.event('ace-tool-confirmation-open', {
    requestId: 'main-chat-request',
    message: '앱 종료 작업을 실행할까요?',
    choices: [
      { id: 'yes', label: '예, 실행' },
      { id: 'no', label: '아니오, 취소' },
    ],
  });
  assert.ok(h.render());
  assert.equal(h.render().props.choices.length, 2);
  h.render().props.onChoice('no');
  await h.flush();
  assert.equal(h.calls.find((c) => c.name === 'respond_voice_choice').args.choiceId, 'no');
  assert.equal(h.calls.filter((c) => c.name === 'submit_voice_tool_call').length, 0);
  await h.event('ace-voice-tool-result', {
    requestId: 'main-chat-request',
    status: 'SUCCESS',
    message: '작업을 취소했어요.',
    choices: [],
  });
  await h.advance(3000);
  assert.equal(h.render(), false);
});

test('confirmation never times out and accepts a spoken number; duplicate click cannot submit again', async () => {
  const h = await harness();
  await h.activate();
  await h.say();
  const request = h.calls.find((c) => c.name === 'submit_voice_tool_call').args.requestId;
  await h.event('ace-voice-tool-result', {
    requestId: request,
    status: 'WAITING_CONFIRMATION',
    message: '선택',
    choices: [
      { id: 'one', label: '첫 파일' },
      { id: 'two', label: '둘째 파일' },
      { id: 'three', label: '셋째 파일' },
    ],
  });
  await h.feed(0, 110);
  assert.ok(h.render());
  assert.equal(h.render().props.choices.length, 3);
  h.recognize({ transcript: '2번', portfolioRequested: false });
  await h.say();
  assert.equal(h.calls.find((c) => c.name === 'respond_voice_choice').args.choiceId, 'two');
  h.render().props.onChoice('two');
  await h.flush();
  assert.equal(h.calls.filter((c) => c.name === 'respond_voice_choice').length, 1);
});

test('manual close invalidates in-flight transcription and choice parser rejects unrelated/negated speech', async () => {
  const h = await harness();
  let resolve;
  h.recognize(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  await h.activate();
  await h.say();
  await h.close();
  resolve({ transcript: '포트폴리오 열어줘', portfolioRequested: true });
  await h.flush();
  assert.equal(h.calls.filter((c) => c.name === 'submit_voice_tool_call').length, 0);
  const choices = [
    { id: 'yes', label: '실행' },
    { id: 'no', label: '취소' },
  ];
  assert.equal(h.policy.resolveVoiceChoice('네.', choices), 'yes');
  assert.equal(h.policy.resolveVoiceChoice('아니요', choices), 'no');
  assert.equal(h.policy.resolveVoiceChoice('3번', choices), null);
  assert.equal(h.policy.resolveVoiceChoice('네라고 하지 않았어', choices), null);
});

test('answering confirmation from the main window stops voice capture before completion', async () => {
  const h = await harness();
  await h.activate();
  await h.say();
  const requestId = h.calls.find((c) => c.name === 'submit_voice_tool_call').args.requestId;
  await h.event('ace-voice-tool-result', {
    requestId,
    status: 'WAITING_CONFIRMATION',
    message: '실행?',
    choices: [
      { id: 'yes', label: '네' },
      { id: 'no', label: '아니오' },
    ],
  });
  assert.equal(h.render().props.recording, true);
  await h.event('ace-voice-tool-result', {
    requestId,
    status: 'EXECUTING',
    message: '실행 중',
    choices: [],
  });
  assert.equal(h.render().props.recording, false);
  await h.event('ace-voice-tool-result', {
    requestId,
    status: 'SUCCESS',
    message: '완료',
    choices: [],
  });
  await h.feed(0.1, 30);
  assert.equal(h.calls.filter((c) => c.name === 'respond_voice_choice').length, 0);
  await h.advance(3000);
  assert.equal(h.render(), false);
});

test('native execution failure starts a silent retry instead of the success close timer', async () => {
  const h = await harness();
  await h.activate();
  await h.say();
  const requestId = h.calls.find((c) => c.name === 'submit_voice_tool_call').args.requestId;
  await h.event('ace-voice-tool-result', {
    requestId,
    status: 'ERROR',
    message: '파일을 찾지 못했어요.',
    choices: [],
  });
  assert.equal(h.render().props.recording, true);
  await h.feed(0, 30);
  assert.ok(h.render(), 'failure must not close at the success deadline');
  await h.feed(0, 20);
  assert.equal(h.render(), false);
});

test('short spoken yes answers confirmation without relaxing normal command speech detection', async () => {
  const h = await harness();
  await h.activate();
  await h.say();
  const requestId = h.calls.find((c) => c.name === 'submit_voice_tool_call').args.requestId;
  await h.event('ace-voice-tool-result', {
    requestId,
    status: 'WAITING_CONFIRMATION',
    message: '실행할까요?',
    choices: [
      { id: 'yes', label: '네' },
      { id: 'no', label: '아니오' },
    ],
  });
  h.recognize({ transcript: '네.', portfolioRequested: false });
  await h.feed(0.08, 2);
  await h.feed(0, 15);
  assert.equal(h.calls.find((c) => c.name === 'respond_voice_choice').args.choiceId, 'yes');
});
