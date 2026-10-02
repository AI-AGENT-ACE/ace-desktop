import { test, expect } from '@playwright/test';

test('Orb 활성화는 마이크를 시작하고 닫을 때 트랙을 정리한다', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem(
      'ace-auth-session',
      JSON.stringify({ accessToken: 'test-access-token', refreshToken: 'test-refresh-token' }),
    );
    let callbackId = 0;
    const callbacks = new Map<number, (event: unknown) => void>();
    const state = {
      requested: 0,
      stopped: 0,
      calls: [] as string[],
      processor: null as null | { onaudioprocess: null | ((event: unknown) => void) },
    };
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
          if (command === 'stop_voice_recording') {
            return {
              recordingId: 'voice_test',
              state: 'READY',
              sampleRate: 48000,
              channels: 1,
              samplesWritten: 48000,
              durationMs: 1000,
            };
          }
          if (command === 'upload_voice_recording')
            return {
              recordingId: 'voice_test',
              state: 'SUCCESS',
              result: { transcript: '테스트' },
            };
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
    class AudioContextMock {
      sampleRate = 48000;
      destination = {};
      createMediaStreamSource() {
        return { connect: () => {}, disconnect: () => {} };
      }
      createScriptProcessor() {
        const processor = {
          onaudioprocess: null as null | ((event: unknown) => void),
          connect: () => {},
          disconnect: () => {},
        };
        state.processor = processor;
        return processor;
      }
      createGain() {
        return { gain: { value: 1 }, connect: () => {}, disconnect: () => {} };
      }
      close() {
        return Promise.resolve();
      }
    }
    Object.assign(window, { AudioContext: AudioContextMock });
  });
  await page.goto('/#voice');
  await expect(page.getByRole('complementary', { name: 'ACE 음성 명령' })).toBeVisible();
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
  await page.evaluate(() => {
    const state = (
      window as unknown as {
        trayVoiceTest: {
          state: { processor: { onaudioprocess: null | ((event: unknown) => void) } };
        };
      }
    ).trayVoiceTest.state;
    state.processor.onaudioprocess?.({
      inputBuffer: { getChannelData: () => new Float32Array(4096).fill(0.1) },
    });
  });
  await page.getByRole('button', { name: /ACE Orb, 클릭하여 녹음 종료/ }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { trayVoiceTest: { state: { calls: string[] } } }).trayVoiceTest.state.calls.includes('upload_voice_recording'))).toBe(true);
  const result = await page.evaluate(
    () =>
      (
        window as unknown as {
          trayVoiceTest: { state: { stopped: number; calls: string[] } };
        }
      ).trayVoiceTest.state,
  );
  expect(result.stopped).toBe(1);
  expect(result.calls).toContain('start_voice_recording');
  expect(result.calls).toContain('stop_voice_recording');
  expect(result.calls).toContain('upload_voice_recording');
  expect(result.calls.indexOf('append_voice_recording_samples')).toBeLessThan(
    result.calls.indexOf('stop_voice_recording'),
  );
  expect(result.calls.indexOf('stop_voice_recording')).toBeLessThan(
    result.calls.indexOf('upload_voice_recording'),
  );
});
