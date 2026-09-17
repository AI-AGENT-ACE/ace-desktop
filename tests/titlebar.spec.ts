import { test, expect } from '@playwright/test';
import { login } from './helpers';

test('ACE 타이틀바가 실제 Tauri API 명령을 호출하고 최대화 상태를 반영', async ({ page }) => {
  await page.addInitScript(() => {
    const state = {
      maximized: false,
      calls: [] as string[],
      actions: [] as { action: string; durationMs: number }[],
    };
    Object.assign(window, {
      isTauri: true,
      titlebarTest: state,
      __TAURI_INTERNALS__: {
        metadata: { currentWindow: { label: 'main' } },
        transformCallback: () => 1,
        invoke: async (command: string, args?: { action: string; durationMs: number }) => {
          state.calls.push(command);
          if (command === 'animate_window_action' && args) {
            state.actions.push(args);
          }
          if (command === 'plugin:window|toggle_maximize') state.maximized = !state.maximized;
          if (command === 'plugin:window|is_maximized') return state.maximized;
          if (command === 'plugin:event|listen') return 1;
        },
      },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
    });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '창 최소화', exact: true }).click();
  await page.getByRole('button', { name: '창 최대화', exact: true }).click();
  await expect(page.getByRole('button', { name: '창 복원', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '창 복원', exact: true }).click();
  await expect(page.getByRole('button', { name: '창 최대화', exact: true })).toBeVisible();
  await page.locator('.titlebar-brand span').dispatchEvent('mousedown', { button: 0, detail: 1 });
  await page.locator('.drag-region').dispatchEvent('mousedown', { button: 0, detail: 1 });
  await page.locator('.drag-region').dispatchEvent('mousedown', { button: 0, detail: 2 });
  await expect(page.getByRole('button', { name: '창 복원', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '창 닫기', exact: true }).click();
  const calls = await page.evaluate(
    () => (window as unknown as { titlebarTest: { calls: string[] } }).titlebarTest.calls,
  );
  expect(calls).not.toContain('animate_window_action');
  expect(calls.filter((c) => c === 'plugin:window|minimize')).toHaveLength(1);
  expect(calls.filter((c) => c === 'plugin:window|toggle_maximize')).toHaveLength(3);
  expect(calls.filter((c) => c === 'hide_ace')).toHaveLength(1);
  expect(calls.filter((c) => c === 'plugin:window|start_dragging')).toHaveLength(2);
  expect(calls).not.toContain('plugin:window|hide');
  const actions = () =>
    page.evaluate(
      () =>
        (
          window as unknown as {
            titlebarTest: { actions: { action: string; durationMs: number }[] };
          }
        ).titlebarTest.actions,
    );
  expect(await actions()).toEqual([]);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.getByRole('button', { name: '창 복원', exact: true }).click();
  await expect(page.getByRole('button', { name: '창 최대화', exact: true })).toBeVisible();
  expect(await actions()).toEqual([]);
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { titlebarTest: { calls: string[] } }).titlebarTest.calls.filter(
          (c) => c === 'plugin:window|toggle_maximize',
        ).length,
    ),
  ).toBe(4);
});

test('브라우저 미리보기 창 제어 안내', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await login(page);
  await page.getByRole('button', { name: '창 최소화', exact: true }).click();
  await expect(page.locator('.toast')).toContainText('Tauri 데스크톱 앱');
  expect(errors).toEqual([]);
});
