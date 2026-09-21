import { test, expect } from '@playwright/test';

test('Orb 활성화는 마이크를 시작하고 닫을 때 트랙을 정리한다', async ({ page }) => {
  await page.addInitScript(() => {
    let callbackId = 0;
    const callbacks = new Map<number, (event: unknown) => void>();
    const state = { requested: 0, stopped: 0, calls: [] as string[] };
    Object.assign(window, {
      trayVoiceTest: { state, callbacks },
      __TAURI_INTERNALS__: {
        transformCallback: (callback: (event: unknown) => void) => {
          callbackId += 1;
          callbacks.set(callbackId, callback);
          return callbackId;
        },
        invoke: async (command: string, args?: { handler?: number }) => {
          state.calls.push(command);
          if (command === 'plugin:event|listen') return args?.handler ?? 0;
          return undefined;
        },
      },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
    });
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: {
        getUserMedia: async () => {
          state.requested += 1;
          return { getTracks: () => [{ stop: () => (state.stopped += 1) }] };
        },
      },
    });
  });
  await page.goto('/#voice');
  await expect(page.getByRole('complementary', { name: '음성 명령 입력' })).toBeVisible();
  await page.evaluate(() => {
    const testState = (
      window as unknown as {
        trayVoiceTest: { callbacks: Map<number, (event: unknown) => void> };
      }
    ).trayVoiceTest;
    testState.callbacks.get(1)?.({ event: 'ace-voice-activate', id: 1, payload: null });
  });
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (
            window as unknown as {
              trayVoiceTest: { state: { requested: number } };
            }
          ).trayVoiceTest.state.requested,
      ),
    )
    .toBe(1);
  await page.getByRole('button', { name: '음성 입력 닫기' }).click();
  const result = await page.evaluate(
    () =>
      (
        window as unknown as {
          trayVoiceTest: { state: { stopped: number; calls: string[] } };
        }
      ).trayVoiceTest.state,
  );
  expect(result.stopped).toBe(1);
  expect(result.calls).toContain('hide_voice_overlay');
});
