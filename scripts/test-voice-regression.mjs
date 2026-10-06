import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import ts from 'typescript';
import axios from 'axios';

const bus = new Map();
async function runtime(label) {
  const storage = () => {
    const data = new Map();
    return {
      getItem: (key) => data.get(key) ?? null,
      setItem: (key, value) => data.set(key, value),
      removeItem: (key) => data.delete(key),
    };
  };
  const context = vm.createContext({
    console,
    setTimeout,
    clearTimeout,
    crypto: globalThis.crypto,
    localStorage: storage(),
    sessionStorage: storage(),
    Event: class {},
    window: { dispatchEvent() {} },
  });
  const cache = new Map();
  const synthetic = (name, exports) =>
    new vm.SyntheticModule(
      Object.keys(exports),
      function () {
        for (const [key, value] of Object.entries(exports)) this.setExport(key, value);
      },
      { context, identifier: name },
    );
  const listeners = new Map();
  cache.set(
    '@tauri-apps/api/core',
    synthetic('core', {
      isTauri: () => false,
      invoke: async () => {
        throw new Error('Unexpected native execution in test');
      },
    }),
  );
  bus.set(label, listeners);
  cache.set(
    '@tauri-apps/api/event',
    synthetic('event', {
      listen: async (event, handler) => {
        listeners.set(event, handler);
        return () => listeners.delete(event);
      },
      emitTo: async (target, event, payload) => {
        void bus.get(target)?.get(event)?.({ payload });
      },
    }),
  );
  cache.set(
    'axios',
    synthetic('axios', {
      default: axios,
      AxiosError: axios.AxiosError,
      CanceledError: axios.CanceledError,
    }),
  );
  cache.set('/src/api/metrics.ts', synthetic('metrics', { recordRequestMetric() {} }));
  async function load(name) {
    if (cache.has(name)) return cache.get(name);
    const source = await readFile(new URL(`..${name}`, import.meta.url), 'utf8');
    const js = ts.transpileModule(source, {
      compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
    }).outputText;
    const mod = new vm.SourceTextModule(js, {
      context,
      identifier: name,
      initializeImportMeta(meta) {
        meta.env = { VITE_API_BASE_URL: 'http://127.0.0.1:3002', DEV: false };
      },
    });
    cache.set(name, mod);
    await mod.link(async (specifier) =>
      load(
        specifier.startsWith('.')
          ? new URL(specifier + '.ts', 'file://' + name).pathname
          : specifier,
      ),
    );
    return mod;
  }
  async function imported(name) {
    const mod = await load(name);
    if (mod.status === 'linked') await mod.evaluate();
    return mod.namespace;
  }
  return { imported, listeners };
}

test('portfolio phrase maps to the exact file without accepting unrelated commands', async () => {
  const { classifyVoiceCommand } = await (
    await runtime('classifier')
  ).imported('/src/features/system-actions/adapters/systemAdapter.ts');
  const command = classifyVoiceCommand('바탕화면에서 포트폴리오 열어줘');
  assert.equal(command.commandType, 'file.open');
  assert.equal(command.arguments.path, '김환성_포트폴리오.pdf');
  for (const phrase of ['포트폴리오 열지마', '다른 파일 열어줘', '포트폴리오 삭제해줘']) {
    assert.notEqual(classifyVoiceCommand(phrase).commandType, 'file.open');
  }
});

test('endpoint preserves initial silence and sentence pauses; ends after speech + 1.5s silence', async () => {
  const { RecordingEndpoint } = await (
    await runtime('endpoint')
  ).imported('/src/features/voice/recording-endpoint.ts');
  const endpoint = new RecordingEndpoint();
  assert.deepEqual(
    [
      endpoint.feed(0, 3000),
      endpoint.feed(0.04, 500),
      endpoint.feed(0, 1000),
      endpoint.feed(0.04, 500),
      endpoint.feed(0, 1400),
      endpoint.feed(0, 100),
    ],
    [null, null, null, null, null, 'silence'],
  );
});
test('empty microphone and continuous noise reach recording limits', async () => {
  const { RecordingEndpoint } = await (
    await runtime('endpoint')
  ).imported('/src/features/voice/recording-endpoint.ts');
  const empty = new RecordingEndpoint(),
    noise = new RecordingEndpoint();
  assert.deepEqual(
    [empty.feed(0, 9900), empty.feed(0, 100), noise.feed(0.03, 29900), noise.feed(0.03, 100)],
    [null, 'timeout', null, 'timeout'],
  );
});
test('separate windows: login after voice creation, expired token refresh, concurrent requests, logout', async () => {
  const main = await runtime('main'),
    voice = await runtime('voice');
  const voiceSession = await voice.imported('/src/api/session.ts');
  const voiceAuth = await voice.imported('/src/api/voice-session.ts');
  const mainSession = await main.imported('/src/api/session.ts');
  const mainClient = await main.imported('/src/api/client.ts');
  let refreshes = 0;
  mainClient.apiClient.defaults.adapter = async (config) => {
    if (config.url === '/auth/refresh') {
      refreshes++;
      await new Promise((resolve) => setTimeout(resolve, 10));
      return {
        data: { accessToken: 'fresh', refreshToken: 'rotated' },
        status: 200,
        headers: {},
        config,
      };
    }
    if (config.headers.Authorization !== 'Bearer fresh') {
      throw new axios.AxiosError('Expired', 'ERR_BAD_REQUEST', config, undefined, {
        data: {},
        status: 401,
        headers: {},
        config,
      });
    }
    return { data: { id: 'user' }, status: 200, headers: {}, config };
  };
  await (await main.imported('/src/api/voice-session.ts')).serveVoiceSession();
  assert.equal(voiceSession.getSession(), null);
  mainSession.setSession({ accessToken: 'expired', refreshToken: 'original' }, false);
  const [token] = await Promise.all([
    voiceAuth.requestVoiceAccessToken(),
    mainClient.apiClient.get('/users/me'),
  ]);
  assert.equal(token, 'fresh');
  assert.equal(refreshes, 1);
  assert.equal(voiceSession.getSession(), null);
  assert.equal(voice.listeners.has('ace-voice-auth-response'), false);
  mainSession.setSession(null);
  await assert.rejects(voiceAuth.requestVoiceAccessToken(), /로그인이 만료/);
  assert.equal(voice.listeners.has('ace-voice-auth-response'), false);
  mainSession.setSession({ accessToken: 'fresh', refreshToken: 'rotated' }, false);
  mainClient.apiClient.defaults.adapter = async (config) => {
    throw new axios.AxiosError('Offline', 'ERR_NETWORK', config);
  };
  await assert.rejects(voiceAuth.requestVoiceAccessToken(), /서버 연결/);
  assert.equal(mainSession.getSession().accessToken, 'fresh');
});
