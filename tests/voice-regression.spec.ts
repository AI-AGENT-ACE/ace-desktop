import { test, expect } from '@playwright/test';

test('speech endpoint waits through initial silence and pauses, then stops after 1.5 seconds', async ({
  page,
}) => {
  await page.goto('/');
  const result = await page.evaluate(async () => {
    // @ts-expect-error Vite serves source modules for browser regression tests.
    const { RecordingEndpoint } = await import('/src/features/voice/recording-endpoint.ts');
    const endpoint = new RecordingEndpoint();
    return [
      endpoint.feed(0, 3000),
      endpoint.feed(0.04, 500),
      endpoint.feed(0, 1000),
      endpoint.feed(0.04, 500),
      endpoint.feed(0, 1400),
      endpoint.feed(0, 100),
    ];
  });
  expect(result).toEqual([null, null, null, null, null, 'silence']);
});

test('empty input and continuous noise cannot record indefinitely', async ({ page }) => {
  await page.goto('/');
  const result = await page.evaluate(async () => {
    // @ts-expect-error Vite source module.
    const { RecordingEndpoint } = await import('/src/features/voice/recording-endpoint.ts');
    const empty = new RecordingEndpoint();
    const noise = new RecordingEndpoint();
    return [
      empty.feed(0, 9900),
      empty.feed(0, 100),
      noise.feed(0.04, 29900),
      noise.feed(0.04, 100),
    ];
  });
  expect(result).toEqual([null, 'timeout', null, 'timeout']);
});

test('already-created voice window obtains main session and refreshes expired access without copying refresh token', async ({
  context,
}) => {
  const main = await context.newPage();
  const voice = await context.newPage();
  const pages = { main, voice };
  let refreshes = 0;
  await context.route('http://127.0.0.1:3002/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/auth/refresh') {
      refreshes++;
      await route.fulfill({ json: { accessToken: 'fresh-access', refreshToken: 'fresh-refresh' } });
    } else if (path === '/users/me') {
      await route.fulfill(
        route.request().headers().authorization === 'Bearer fresh-access'
          ? { json: { id: 'test-user' } }
          : { status: 401, json: { code: 'UNAUTHENTICATED' } },
      );
    } else await route.fulfill({ json: { items: [] } });
  });
  for (const [label, page] of Object.entries(pages)) {
    await page.exposeBinding('voiceTestInvoke', async (_source, command: string, args: any) => {
      if (command === 'plugin:event|emit_to') {
        const target = pages[args.target.label as keyof typeof pages];
        await target.evaluate(
          ({ event, payload }) => {
            const state = (window as any).__voiceTest;
            const handler = state.events[event];
            if (handler) state.callbacks[handler]({ event, payload, id: handler });
          },
          { event: args.event, payload: args.payload },
        );
      }
    });
    await page.goto(label === 'voice' ? '/#voice' : '/');
    await page.evaluate(() => {
      const state = { next: 1, callbacks: {} as any, events: {} as any };
      (window as any).__voiceTest = state;
      (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
      (window as any).__TAURI_INTERNALS__ = {
        transformCallback: (fn: any) => {
          const id = state.next++;
          state.callbacks[id] = fn;
          return id;
        },
        invoke: async (command: string, args: any) => {
          if (command === 'plugin:event|listen') {
            state.events[args.event] = args.handler;
            return args.handler;
          }
          if (command === 'plugin:event|unlisten') {
            delete state.events[args.event];
            return;
          }
          return (window as any).voiceTestInvoke(command, args);
        },
      };
    });
  }
  await main.evaluate(async () => {
    // @ts-expect-error Vite source module.
    const { setSession } = await import('/src/api/session.ts');
    setSession({ accessToken: 'expired-access', refreshToken: 'original-refresh' }, false);
    // @ts-expect-error Vite source module.
    const { serveVoiceSession } = await import('/src/api/voice-session.ts');
    await serveVoiceSession();
  });
  const result = await voice.evaluate(async () => {
    // @ts-expect-error Vite source module.
    const { getSession } = await import('/src/api/session.ts');
    // @ts-expect-error Vite source module.
    const { requestVoiceAccessToken } = await import('/src/api/voice-session.ts');
    return { before: getSession(), token: await requestVoiceAccessToken(), after: getSession() };
  });
  expect(result).toEqual({ before: null, token: 'fresh-access', after: null });
  expect(refreshes).toBe(1);
  await main.evaluate(async () => {
    // @ts-expect-error Vite source module.
    const { setSession } = await import('/src/api/session.ts');
    setSession(null);
  });
  expect(
    await voice.evaluate(async () => {
      // @ts-expect-error Vite source module.
      const { requestVoiceAccessToken } = await import('/src/api/voice-session.ts');
      try {
        await requestVoiceAccessToken();
        return 'unexpected success';
      } catch (error) {
        return (error as Error).message;
      }
    }),
  ).toContain('로그인이 만료');
});
