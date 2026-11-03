import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import ts from 'typescript';

async function harness({ cloud = false, chime = false } = {}) {
  const hooks = [];
  let cursor = 0;
  let now = 0;
  let timerId = 0;
  const timers = new Map(),
    listeners = new Map(),
    calls = [],
    processors = [],
    effects = [];
  let captureId = null;
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
    currentTime = 0;
    state = 'running';
    async resume() {}
    async close() {
      this.state = 'closed';
    }
    createMediaStreamSource() {
      return {
        connect() {
          calls.push({ name: 'capture_connected' });
        },
        disconnect() {},
      };
    }
    createGain() {
      return {
        gain: {
          value: 0,
          setValueAtTime() {},
          linearRampToValueAtTime() {},
          exponentialRampToValueAtTime() {},
        },
        connect() {},
        disconnect() {},
      };
    }
    createOscillator() {
      return {
        frequency: { value: 0 },
        connect() {},
        start() {
          calls.push({ name: 'wake_chime_start' });
        },
        stop() {},
      };
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
    localStorage: { getItem: (key) => (key === 'ace-wake-chime' && !chime ? 'off' : null) },
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
      emitTo: async (target, event, payload) => { calls.push({ command: 'emitTo', args: { target, event, payload } }); },
      listen: async (name, fn) => {
        listeners.set(name, fn);
        return () => listeners.delete(name);
      },
    },
    '@tauri-apps/api/core': {
      invoke: async (name, args) => {
        calls.push({ name, args });
        if (name === 'start_audio_input') {
          captureId = args.sessionId;
          calls.push({ name: 'capture_connected' });
          return 48000;
        }
        if (name === 'stop_audio_input') {
          captureId = null;
        }
        if (name === 'stop_voice_recording') return { recordingId: `recording-${calls.length}` };
        if (name === 'transcribe_local_portfolio')
          return typeof result === 'function' ? result() : result;
        if (name === 'upload_voice_recording')
          return { result: typeof result === 'function' ? await result() : result };
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
          meta.env = { DEV: false, VITE_VOICE_MODE: cloud ? 'cloud' : 'local-portfolio' };
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
      if (captureId)
        listeners.get('ace-audio-input')?.({
          payload: {
            sessionId: captureId,
            sampleRate: 48000,
            rms,
            samples: Array.from(new Float32Array(4800).fill(rms)),
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

test('microphone disconnect stops recording without uploading or switching to another device', async () => {
  const h = await harness({ cloud: true });
  await h.activate();
  await h.feed(0.08, 5);
  const input = h.calls.find((c) => c.name === 'start_audio_input');
  await h.event('ace-audio-input-error', {
    sessionId: input.args.sessionId,
    message: '마이크 연결이 끊겼습니다.',
  });
  assert.equal(h.render().props.recording, false);
  assert.equal(h.render().props.visualState, 'ERROR');
  assert.equal(h.calls.filter((c) => c.name === 'upload_voice_recording').length, 0);
  assert.equal(h.calls.filter((c) => c.name === 'start_audio_input').length, 1);
  await h.feed(0.08, 10);
  assert.equal(h.calls.filter((c) => c.name === 'upload_voice_recording').length, 0);
});

test('microphone change cancels the old capture and allows restarting without closing Orb', async () => {
  const h = await harness();
  await h.activate();
  await h.event('ace-audio-input-changed', null);
  assert.equal(h.render().props.recording, false);
  assert.equal(h.render().props.canStartRecording, true);
  h.render().props.onStartRecording();
  await h.flush();
  assert.equal(h.render().props.recording, true);
  assert.equal(h.calls.filter((c) => c.name === 'start_audio_input').length, 2);
});

test('chatbot answer stays visible, Orb click starts one new recording and X closes it', async () => {
  const h = await harness({ cloud: true });
  h.recognize({
    transcript: '서울 여행 알려줘',
    type: 'message',
    content: '### 서울 여행<br>**준비물**',
  });
  await h.activate();
  await h.say();
  assert.equal(h.render().props.isAnswer, true);
  assert.equal(h.render().props.canStartRecording, true);
  assert.equal(h.render().props.recording, false);
  await h.advance(60000);
  assert.ok(h.render());
  const callback = h.render().props.onStartRecording;
  callback();
  callback();
  await h.flush();
  assert.equal(h.calls.filter((c) => c.name === 'start_voice_recording').length, 2);
  assert.equal(h.render().props.recording, true);
  await h.close();
  assert.equal(h.render(), false);
});

test('silent follow-up in chatbot mode returns to idle without closing the Orb', async () => {
  const h = await harness({ cloud: true });
  h.recognize({ transcript: '안녕', type: 'message', content: '안녕하세요' });
  await h.activate();
  await h.say();
  h.render().props.onStartRecording();
  await h.flush();
  await h.feed(0, 100);
  assert.ok(h.render());
  assert.equal(h.render().props.canStartRecording, true);
  assert.equal(h.render().props.recording, false);
  await h.close();
  assert.equal(h.render(), false);
});

test('readiness chime starts after the microphone writer and PCM callback are connected', async () => {
  const h = await harness({ chime: true });
  await h.activate();
  const names = h.calls.map((c) => c.name);
  assert.ok(names.indexOf('start_voice_recording') >= 0);
  assert.ok(names.indexOf('capture_connected') >= 0);
  assert.ok(names.indexOf('start_voice_recording') > names.indexOf('capture_connected'));
  assert.ok(names.indexOf('wake_chime_start') > names.indexOf('capture_connected'));
});

test('closing a cloud transcription ignores its delayed chatbot answer', async () => {
  const h = await harness({ cloud: true });
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
  resolve({ transcript: '안녕', type: 'message', content: '늦은 답변' });
  await h.flush();
  assert.equal(h.render(), false);
});

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

test('confirmation accepts a spoken number; duplicate click cannot submit again', async () => {
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
  await h.feed(0, 10);
  assert.ok(h.render());
  assert.equal(h.render().props.choices.length, 3);
  h.recognize({ transcript: '2번', portfolioRequested: false });
  await h.say();
  assert.equal(h.calls.find((c) => c.name === 'respond_voice_choice').args.choiceId, 'two');
  h.render().props.onChoice('two');
  await h.flush();
  assert.equal(h.calls.filter((c) => c.name === 'respond_voice_choice').length, 1);
});

for (const transcript of ['취소', '아니오', '2번', '취소해 주세요', '아니요 아니요']) {
  test(`spoken refusal reaches the existing cancellation IPC: ${transcript}`, async () => {
    const h = await harness();
    await h.event('ace-tool-confirmation-open', {
      requestId: 'refusal-test',
      message: '닫을까요?',
      choices: [
        { id: 'yes', label: '닫기' },
        { id: 'no', label: '취소' },
      ],
    });
    h.recognize({ transcript, portfolioRequested: false });
    await h.say();
    const responses = h.calls.filter((c) => c.name === 'respond_voice_choice');
    assert.equal(responses.length, 1);
    assert.equal(responses[0].args.choiceId, 'no');
    assert.equal(h.calls.filter((c) => c.name === 'submit_voice_tool_call').length, 0);
  });
}
for (const fails of [false, true]) {
  test(`confirmation recognition failure cancels after three attempts: ${fails}`, async () => {
    const h = await harness();
    await h.event('ace-tool-confirmation-open', {
      requestId: 'failed-recognition',
      message: '닫을까요?',
      choices: [
        { id: 'yes', label: '닫기' },
        { id: 'no', label: '취소' },
      ],
    });
    h.recognize(
      fails
        ? () => {
            throw new Error('LOCAL_STT_FAILED');
          }
        : { transcript: '확실하지 않은 말', portfolioRequested: false },
    );
    await h.say();
    await h.say();
    assert.equal(h.calls.filter((c) => c.name === 'respond_voice_choice').length, 0);
    await h.say();
    assert.equal(h.calls.find((c) => c.name === 'respond_voice_choice')?.args.choiceId, 'no');
  });
}
test('silent confirmation cancels rather than leaving an executable request pending', async () => {
  const h = await harness();
  await h.event('ace-tool-confirmation-open', {
    requestId: 'silent-refusal',
    message: '닫을까요?',
    choices: [
      { id: 'yes', label: '닫기' },
      { id: 'no', label: '취소' },
    ],
  });
  await h.feed(0, 51);
  assert.equal(h.calls.find((c) => c.name === 'respond_voice_choice')?.args.choiceId, 'no');
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
