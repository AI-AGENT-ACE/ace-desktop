import { test, expect, type Page } from '@playwright/test';
import { toolPresentations, toolGroups } from '../src/features/system-actions/toolPresentation';

test('권한은 한국어 8개 그룹으로 표시하고 저장 계약은 유지', async ({ page }) => {
  await mock(page);
  const names = Object.keys(toolPresentations);
  await page.route('**/tools', (r) =>
    r.fulfill({
      json: names.map((name) => ({ name, defaultPolicy: 'ASK', systemConfirmation: false })),
    }),
  );
  await page.route('**/settings/permissions', (r) => r.fulfill({ json: [] }));
  const saved: unknown[] = [];
  await page.route('**/settings/permissions/app.focus', (r) => {
    saved.push({ method: r.request().method(), body: r.request().postDataJSON() });
    return r.fulfill({ json: { toolName: 'app.focus', policy: 'DENY' } });
  });
  await page.getByRole('button', { name: '설정 열기' }).click();
  await page.getByRole('button', { name: /권한 설정하기/ }).click();
  const list = page.locator('.permission-list');
  for (const group of toolGroups)
    await expect(list.getByRole('heading', { name: group, exact: true })).toBeVisible();
  await expect(list.locator('fieldset')).toHaveCount(24);
  for (const name of names) expect(await list.innerText()).not.toContain(name);
  for (const policy of ['DENY', 'ASK', 'ALWAYS_ALLOW'])
    expect(await list.innerText()).not.toContain(policy);
  const row = list.getByRole('group', { name: '앱 창 앞으로 가져오기', exact: true });
  await row.getByRole('radio', { name: '차단', exact: true }).check();
  await expect.poll(() => saved.length).toBe(1);
  expect(saved[0]).toEqual({ method: 'PUT', body: { policy: 'DENY' } });
  await page.setViewportSize({ width: 600, height: 800 });
  const clipboard = list.getByRole('group', { name: '클립보드에 텍스트 복사', exact: true });
  await clipboard.scrollIntoViewIfNeeded();
  expect(
    await clipboard.evaluate((el) => {
      const description = el.querySelector('.permission-description')!.getBoundingClientRect();
      const buttons = el.querySelector('.permission-choices')!.getBoundingClientRect();
      return description.bottom <= buttons.top && el.scrollWidth <= el.clientWidth;
    }),
  ).toBe(true);
});
test('개발 패널은 AI 없이 서버 권한·실행 등록·Rust 승인 경로로 창 종료 요청', async ({ page }) => {
  const { errors } = await mock(page);
  await page.route('**/tools', (r) =>
    r.fulfill({ json: [{ name: 'app.close', defaultPolicy: 'ASK', systemConfirmation: true }] }),
  );
  let aiCalls = 0;
  const events: string[] = [];
  page.on('request', (r) => {
    if (new URL(r.url()).pathname === '/agent/turns') aiCalls++;
  });
  await page.route('**/settings/permissions', (r) =>
    r.fulfill({
      json: [{ toolName: 'app.close', denied: false, requiresConfirmation: true, policy: 'ASK' }],
    }),
  );
  await page.route('**/agent/executions', (r) =>
    r.fulfill({
      json: {
        executionId: 'cc096a32-d3b9-4f53-8d2c-f39580ea9344',
        requestId: '8fb1b9ae-51cf-4cea-b996-a3cafb8597e1',
        ticket: 'test-only',
        denied: false,
      },
    }),
  );
  await page.route('**/agent/executions/events', (r) => {
    events.push(r.request().postDataJSON().state);
    return r.fulfill({ json: {} });
  });
  await page.getByText('Native 테스트 · 개발 전용', { exact: true }).click();
  await expect(page.getByTestId('native-dev-panel')).toContainText('http://127.0.0.1:3002');
  await page.getByLabel('테스트 작업', { exact: true }).selectOption('app.close');
  await page.getByRole('button', { name: '테스트 요청', exact: true }).click();
  await page
    .getByRole('dialog', { name: '작업할 창 선택' })
    .getByRole('button', { name: '메모장 · 문서 2', exact: true })
    .click();
  expect(
    await page.evaluate(
      () =>
        (window as any).inputWindowTest.calls.filter(
          (c: any) => c.command === 'execute_native_tool',
        ).length,
    ),
  ).toBe(0);
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect.poll(() => events.includes('SUCCEEDED')).toBe(true);
  const calls = await page.evaluate(() => (window as any).inputWindowTest.calls);
  expect(calls.some((c: any) => c.command === 'authorize_native_tool')).toBe(true);
  expect(
    calls.find((c: any) => c.command === 'execute_native_tool').args.arguments.targetWindowId,
  ).toBe('window-2');
  expect(events).toContain('WAITING_APPROVAL');
  expect(events).toContain('APPROVED');
  expect(aiCalls).toBe(0);
  expect(errors).toEqual([]);
});

test('개발 패널은 차단된 명령과 테스트 폴더 밖 파일 요청을 실행하지 않음', async ({ page }) => {
  await mock(page);
  await page.route('**/tools', (r) =>
    r.fulfill({ json: [{ name: 'app.close', defaultPolicy: 'ASK', systemConfirmation: true }] }),
  );
  await page.route('**/settings/permissions', (r) =>
    r.fulfill({
      json: [{ toolName: 'app.close', denied: true, requiresConfirmation: false, policy: 'DENY' }],
    }),
  );
  let registration = 0;
  page.on('request', (r) => {
    if (new URL(r.url()).pathname === '/agent/executions') registration++;
  });
  await page.getByText('Native 테스트 · 개발 전용', { exact: true }).click();
  await page.getByLabel('테스트 작업', { exact: true }).selectOption('app.close');
  await page.getByRole('button', { name: '테스트 요청', exact: true }).click();
  await expect(
    page.getByText('권한 설정에서 차단되었습니다.', { exact: true }),
  ).toBeVisible();
  await page.getByLabel('테스트 작업', { exact: true }).selectOption('file.copy');
  await page.getByLabel('테스트 폴더 이름').fill('../개인파일');
  await page.getByRole('button', { name: '테스트 요청', exact: true }).click();
  await expect(page.getByTestId('native-dev-panel').getByRole('alert')).toBeVisible();
  expect(registration).toBe(0);
  expect(
    await page.evaluate(() =>
      (window as any).inputWindowTest.calls.some((c: any) =>
        ['prepare_native_tool', 'execute_native_tool'].includes(c.command),
      ),
    ),
  ).toBe(false);
});
test('미등록 기능은 권한 변경을 안내하지 않고 실행하지 않음', async ({ page }) => {
  await mock(page);
  await page.route('**/tools', (r) => r.fulfill({ json: [] }));
  await page.getByText('Native 테스트 · 개발 전용', { exact: true }).click();
  await page.getByLabel('테스트 작업', { exact: true }).selectOption('app.close');
  await page.getByRole('button', { name: '테스트 요청', exact: true }).click();
  await expect(page.getByText('현재 사용할 수 없는 기능입니다.', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '권한 설정으로 이동' })).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).inputWindowTest.calls.some((c: any) =>
    ['prepare_native_tool', 'execute_native_tool'].includes(c.command)))).toBe(false);
});

test('권한 차단 안내에서 해당 기능의 설정으로 이동', async ({ page }) => {
  await mock(page);
  await page.route('**/tools', (r) => r.fulfill({ json: [{ name: 'app.close', defaultPolicy: 'DENY', systemConfirmation: true }] }));
  await page.route('**/settings/permissions', (r) => r.fulfill({ json: [] }));
  await page.getByText('Native 테스트 · 개발 전용', { exact: true }).click();
  await page.getByLabel('테스트 작업', { exact: true }).selectOption('app.close');
  await page.getByRole('button', { name: '테스트 요청', exact: true }).click();
  await page.getByRole('button', { name: '권한 설정으로 이동' }).click();
  const row = page.getByRole('group', { name: '앱 창 닫기', exact: true });
  await expect(row).toBeVisible();
  await expect(row.getByRole('radio', { name: '차단', exact: true })).toBeChecked();
});
for (const code of ['UNKNOWN_TOOL', 'TOOL_PERMISSION_DENIED']) {
  test(`서버 오류 ${code}는 사용자 안내로 변환`, async ({ page }) => {
    await mock(page);
    await page.route('**/tools', (r) => r.fulfill({ status: 400, json: { code, message: 'Technical detail app.close' } }));
    await page.getByText('Native 테스트 · 개발 전용', { exact: true }).click();
    await page.getByLabel('테스트 작업', { exact: true }).selectOption('app.close');
    await page.getByRole('button', { name: '테스트 요청', exact: true }).click();
    await expect(page.locator('.toast')).toContainText(code === 'UNKNOWN_TOOL' ? '현재 사용할 수 없는 기능입니다.' : '권한 설정에서 차단되었습니다.');
    await expect(page.locator('.toast')).not.toContainText('app.close');
    await expect(page.getByRole('button', { name: '권한 설정으로 이동' })).toHaveCount(code === 'UNKNOWN_TOOL' ? 0 : 1);
  });
}
for (const theme of ['light', 'dark']) {
  test(`권한 알림 버튼 대비와 키보드 포커스 ${theme}`, async ({ page }, testInfo) => {
    await page.addInitScript((value) => localStorage.setItem('ace-theme', value), theme);
    await mock(page);
    await page.route('**/tools', (r) => r.fulfill({ json: [{ name: 'app.close', defaultPolicy: 'DENY', systemConfirmation: true }] }));
    await page.route('**/settings/permissions', (r) => r.fulfill({ json: [] }));
    await page.getByText('Native 테스트 · 개발 전용', { exact: true }).click();
    await page.getByLabel('테스트 작업', { exact: true }).selectOption('app.close');
    await page.getByRole('button', { name: '테스트 요청', exact: true }).click();
    const toast = page.locator('.toast');
    await expect(toast).toContainText('권한 설정에서 차단되었습니다.');
    const buttons = toast.getByRole('button');
    for (let i = 0; i < 2; i++) {
      const button = buttons.nth(i);
      for (const state of ['base', 'hover', 'pressed', 'focus']) {
        if (state === 'hover') await button.hover();
        if (state === 'pressed') await page.mouse.down();
        if (state === 'focus') {
          await page.mouse.move(0, 0);
          await button.focus();
          await page.keyboard.press('Tab');
          await page.keyboard.press('Shift+Tab');
          await expect(button).toBeFocused();
          expect(await button.evaluate(el => getComputedStyle(el).outlineStyle)).toBe('solid');
        }
        const contrast = await button.evaluate(el => {
          const style = getComputedStyle(el);
          const luminance = (color: string) => color.match(/[\d.]+/g)!.slice(0, 3).map(Number).map(v => {
            v /= 255; return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
          }).reduce((sum, v, index) => sum + v * [0.2126, 0.7152, 0.0722][index], 0);
          const fg = luminance(style.color), bg = luminance(style.backgroundColor);
          const svg = el.querySelector('svg');
          return { ratio: (Math.max(fg, bg) + 0.05) / (Math.min(fg, bg) + 0.05), icon: !svg || getComputedStyle(svg).stroke === style.color };
        });
        expect(contrast.ratio).toBeGreaterThanOrEqual(4.5);
        expect(contrast.icon).toBe(true);
        await toast.screenshot({ path: testInfo.outputPath(`${theme}-${i}-${state}.png`) });
        if (state === 'pressed') { await page.mouse.move(0, 0); await page.mouse.up(); }
      }
    }
    await toast.getByRole('button', { name: '알림 닫기' }).click();
    await expect(toast).toHaveCount(0);
  });
}
for (const scenario of ['현재 삭제', '다른 대화 삭제', '삭제 실패', '삭제 중 이동', '늦은 메시지 조회', '늦은 AI 응답']) {
  test(`대화 삭제 상태 정리: ${scenario}`, async ({ page }) => {
    await mock(page);
    const rows = ['a', 'b'].map(id => ({ id, title: `대화 ${id}`, isPinned: false, createdAt: '', updatedAt: '' }));
    let finishDelete: () => void = () => {};
    let finishMessages: () => void = () => {};
    let deleting = false;
    let creates = 0;
    let aiRequested = false;
    let finishAi: () => void = () => {};
    await page.route('**/agent/turns', async r => {
      aiRequested = true;
      await new Promise<void>(resolve => { finishAi = resolve; });
      await r.fulfill({ json: { conversationId: 'a', message: { id: 'late-ai', conversationId: 'a', role: 'ASSISTANT', content: '늦은 AI 답변', attachments: [], createdAt: '' }, toolCalls: [] } }).catch(() => {});
    });
    await page.route('**/conversations/a/messages', r => r.fulfill({ json: { id: 'sent-a', conversationId: 'a', role: 'USER', content: '작성 중인 입력', attachments: [], createdAt: '' } }));
    await page.route('**/conversations?*', r => r.fulfill({ json: { items: rows, hasMore: false, nextCursor: null } }));
    await page.route('**/conversations', r => {
      if (r.request().method() === 'POST') creates++;
      return r.fulfill({ json: { items: rows, hasMore: false, nextCursor: null } });
    });
    await page.route('**/conversations/*/messages?*', async r => {
      const id = new URL(r.request().url()).pathname.split('/')[2];
      if (scenario === '늦은 메시지 조회' && id === 'a') await new Promise<void>(resolve => { finishMessages = resolve; });
      await r.fulfill({ json: { items: [{ id: `msg-${id}`, conversationId: id, role: 'USER', content: `본문 ${id}`, attachments: [], createdAt: '' }], hasMore: false, nextCursor: null } }).catch(() => {});
    });
    await page.route('**/conversations/a', async r => {
      if (r.request().method() !== 'DELETE') return r.fulfill({ json: rows[0] });
      deleting = true;
      if (scenario === '삭제 중 이동') await new Promise<void>(resolve => { finishDelete = resolve; });
      await r.fulfill(scenario === '삭제 실패' ? { status: 500, json: { code: 'INTERNAL_SERVER_ERROR', message: '삭제 실패 안내' } } : { status: 204 });
    });
    await page.reload();
    const selected = scenario === '다른 대화 삭제' ? 'b' : 'a';
    await page.getByRole('button', { name: `대화 ${selected}`, exact: true }).click();
    if (scenario !== '늦은 메시지 조회') await expect(page.getByText(`본문 ${selected}`, { exact: true })).toBeVisible();
    await page.getByRole('textbox', { name: '메시지', exact: true }).fill('작성 중인 입력');
    if (scenario === '늦은 AI 응답') {
      await page.getByRole('button', { name: '메시지 보내기', exact: true }).click();
      await expect.poll(() => aiRequested).toBe(true);
    }
    await page.getByRole('button', { name: '대화 a 메뉴' }).click();
    await page.getByRole('button', { name: '삭제', exact: true }).click();
    await expect.poll(() => deleting).toBe(true);
    if (scenario === '삭제 중 이동') {
      await page.getByRole('button', { name: '대화 b', exact: true }).click();
      await page.getByRole('textbox', { name: '메시지', exact: true }).fill('새 대화 입력');
      finishDelete();
    }
    if (scenario === '삭제 실패') {
      await expect(page.locator('.toast')).toContainText('삭제 실패 안내');
      await expect(page.getByRole('button', { name: '대화 a', exact: true })).toBeVisible();
      await expect(page.getByText('본문 a', { exact: true })).toBeVisible();
      await expect(page.getByRole('textbox', { name: '메시지', exact: true })).toHaveValue('작성 중인 입력');
    } else {
      await expect(page.getByRole('button', { name: '대화 a', exact: true })).toHaveCount(0);
      finishMessages();
      finishAi();
      if (scenario === '다른 대화 삭제' || scenario === '삭제 중 이동') {
        await expect(page.getByText('본문 b', { exact: true })).toBeVisible();
        await expect(page.getByRole('textbox', { name: '메시지', exact: true })).toHaveValue(scenario === '삭제 중 이동' ? '새 대화 입력' : '작성 중인 입력');
      } else {
        await expect(page.locator('.conversation.selected')).toHaveCount(0);
        await expect(page.getByText('본문 a', { exact: true })).toHaveCount(0);
        await expect(page.getByRole('textbox', { name: '메시지', exact: true })).toHaveValue('');
        await expect(page.getByText('늦은 AI 답변', { exact: true })).toHaveCount(0);
      }
    }
    expect(creates).toBe(0);
  });
}
for (const fail of [false, true]) {
  test(`휴지통 취소 후 영구 삭제는 일반 목록 유지: ${fail ? '실패' : '성공'}`, async ({ page }, testInfo) => {
    await mock(page);
    const rows = Array.from({ length: 25 }, (_, i) => ({ id: `keep-${i}`, title: `유지 대화 ${i}`, isPinned: false, createdAt: '', updatedAt: '' }));
    const deleted = { id: 'trash-test', title: '삭제 테스트', isPinned: false, createdAt: '', updatedAt: '', deletedAt: new Date().toISOString(), remainingDays: 30, isRestorable: true };
    let removed = false, permanent = false, deletes = 0, lists = 0, trashLists = 0;
    await page.route('**/conversations?*', r => {
      lists++;
      return r.fulfill({ json: { items: removed ? rows : [deleted, ...rows], hasMore: false, nextCursor: null } });
    });
    await page.route('**/conversations/trash-test', r => { removed = true; return r.fulfill({ status: 204 }); });
    await page.route('**/conversations/trash?*', r => {
      trashLists++;
      return r.fulfill({ json: { items: permanent ? [] : [deleted], hasMore: false, nextCursor: null } });
    });
    await page.route('**/conversations/trash-test/permanent', r => {
      deletes++;
      if (!fail) permanent = true;
      return r.fulfill(fail ? { status: 500, json: { message: '영구 삭제 실패' } } : { status: 204 });
    });
    await page.route('**/conversations/keep-0/messages?*', r => r.fulfill({ json: { items: [{ id: 'kept-message', conversationId: 'keep-0', role: 'USER', content: '유지할 메시지', attachments: [], createdAt: '' }], hasMore: false, nextCursor: null } }));
    await page.reload();
    await page.getByRole('button', { name: '유지 대화 0', exact: true }).click();
    await expect(page.getByText('유지할 메시지', { exact: true })).toBeVisible();
    await page.getByRole('textbox', { name: '메시지', exact: true }).fill('보존할 작성 내용');
    await page.getByRole('button', { name: '삭제 테스트 메뉴' }).click();
    await page.getByRole('button', { name: '삭제', exact: true }).click();
    await expect(page.getByRole('button', { name: '삭제 테스트', exact: true })).toHaveCount(0);
    await page.locator('.conversation-list').evaluate(el => { el.scrollTop = 180; });
    const scroll = await page.locator('.conversation-list').evaluate(el => el.scrollTop);
    await page.getByRole('button', { name: '설정 열기' }).click();
    await page.getByRole('button', { name: '휴지통 보기' }).click();
    const baseline = { lists, trashLists };
    await page.evaluate(() => {
      const list = document.querySelector('.conversation-list')!;
      (window as any).listOriginal = list.querySelector('.conversation');
      (window as any).listFlicker = false;
      new MutationObserver(records => {
        if (list.querySelector('.skeletons') || records.some(r => [...r.removedNodes].some(n => n === (window as any).listOriginal || (n instanceof Element && n.contains((window as any).listOriginal))))) (window as any).listFlicker = true;
      }).observe(list, { childList: true, subtree: true });
    });
    await page.getByRole('button', { name: '삭제 테스트 영구 삭제' }).click();
    await page.getByRole('button', { name: '취소', exact: true }).click();
    expect(deletes).toBe(0);
    expect({ lists, trashLists }).toEqual(baseline);
    await page.getByRole('button', { name: '삭제 테스트 영구 삭제' }).click();
    await page.getByRole('button', { name: '영구 삭제', exact: true }).click();
    if (fail) await expect(page.getByRole('alert')).toContainText('영구 삭제 실패');
    else await expect(page.getByText('휴지통이 비어 있습니다', { exact: true })).toBeVisible();
    expect(deletes).toBe(1);
    expect({ lists, trashLists }).toEqual(baseline);
    expect(await page.evaluate(() => (window as any).listFlicker)).toBe(false);
    expect(await page.locator('.conversation-list').evaluate(el => el.scrollTop)).toBe(scroll);
    await page.screenshot({ path: testInfo.outputPath('trash-result.png') });
    await page.getByRole('button', { name: '닫기', exact: true }).click();
    await expect(page.getByRole('textbox', { name: '메시지', exact: true })).toHaveValue('보존할 작성 내용');
    await expect(page.getByText('유지할 메시지', { exact: true })).toBeVisible();
    await expect(page.locator('.conversation.selected')).toContainText('유지 대화 0');
    if (fail) {
      await page.getByRole('button', { name: '설정 열기' }).click();
      await page.getByRole('button', { name: '휴지통 보기' }).click();
      await expect(page.getByRole('button', { name: '삭제 테스트 영구 삭제' })).toBeVisible();
    }
  });
}
test('개발 모드에서는 업데이트 IPC를 호출하지 않음', async ({ page }) => {
  await mock(page);
  await page.getByRole('button', { name: '설정 열기' }).click();
  await expect(page.getByText('개발 모드에서는 앱 업데이트를 사용할 수 없습니다.')).toBeVisible();
  await expect(page.getByRole('button', { name: '업데이트 확인', exact: true })).toBeDisabled();
  await page.waitForTimeout(1200);
  expect(
    await page.evaluate(() =>
      (window as any).inputWindowTest.calls.filter((c: any) =>
        ['update_status', 'check_app_update', 'install_app_update'].includes(c.command),
      ),
    ),
  ).toEqual([]);
});
test('프로필 실패 입력 유지 및 저장 후 사이드바와 재접속 반영', async ({ page }) => {
  await mock(page);
  let displayName = '기존 이름';
  let fail = true;
  let patches = 0;
  await page.route('**/users/me', async (route) => {
    if (route.request().method() === 'PATCH') {
      patches++;
      if (fail) {
        await route.fulfill({ status: 500, json: { message: 'failed' } });
        return;
      }
      displayName = route.request().postDataJSON().displayName;
    }
    await route.fulfill({ json: { id: 'user-1', email: 'test@example.com', displayName } });
  });
  await page.reload();
  await page.getByRole('button', { name: '설정 열기' }).click();
  await expect(page.getByLabel('표시 이름')).toHaveValue('기존 이름');
  await page.getByLabel('표시 이름').fill('새 이름');
  await page.getByRole('button', { name: '프로필 저장' }).click();
  await expect(page.getByLabel('표시 이름')).toHaveValue('새 이름');
  await expect(page.getByRole('button', { name: '프로필 저장' })).toBeEnabled();
  fail = false;
  await page.getByRole('button', { name: '프로필 저장' }).click();
  await expect(page.getByText('프로필을 저장했습니다.')).toBeVisible();
  await expect(page.locator('aside')).toContainText('새 이름');
  expect(patches).toBe(2);
  await page.reload();
  await expect(page.locator('aside')).toContainText('새 이름');
});
async function mock(page: Page, stale = false) {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.addInitScript(
    ({ stale }) => {
      sessionStorage.setItem(
        'ace-auth-session',
        JSON.stringify({
          accessToken: 'test-only',
          refreshToken: 'test-only',
          tokenType: 'Bearer',
          expiresIn: 900,
          refreshExpiresAt: '',
        }),
      );
      const state = {
        next: 1,
        callbacks: {} as any,
        events: {} as any,
        calls: [] as any[],
        stale,
        singleWindow: false,
        sessionId: '',
        settings: { deviceId: null as string | null, allowDefaultFallback: false },
        devices: [
          { id: 'usb', name: '테스트 USB 마이크' },
          { id: 'builtin', name: '테스트 내장 마이크' },
        ],
      };
      const status = () => ({
        settings: state.settings,
        devices: state.devices,
        currentDeviceId: state.settings.deviceId || 'builtin',
        currentDeviceName:
          state.devices.find((d) => d.id === (state.settings.deviceId || 'builtin'))?.name ?? null,
      });
      Object.assign(window, {
        isTauri: true,
        inputWindowTest: state,
        __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
        __TAURI_INTERNALS__: {
          metadata: { currentWindow: { label: 'main' } },
          transformCallback: (fn: any) => {
            const id = state.next++;
            state.callbacks[id] = fn;
            return id;
          },
          invoke: async (command: string, args: any) => {
            state.calls.push({ command, args });
            if (command === 'plugin:event|listen') {
              state.events[args.event] = args.handler;
              return args.handler;
            }
            if (command === 'plugin:event|unlisten') {
              if (state.events[args.event] === args.eventId) delete state.events[args.event];
              return;
            }
            if (command === 'get_wake_word_setup_status')
              return {
                state: 'IDLE',
                settings: {
                  enabled: false,
                  setupCompleted: false,
                  referenceExists: false,
                  defaultAvailable: false,
                  modelSource: null,
                  sensitivity: 'standard',
                },
              };
            if (command === 'get_wake_word_status') return { state: 'disabled', enabled: false };
            if (command === 'get_audio_input_status') return status();
            if (command === 'set_audio_input') {
              state.settings = args.preference;
              return status();
            }
            if (command === 'start_audio_input') {
              state.sessionId = args.sessionId;
              return 16000;
            }
            if (command === 'authorize_native_tool') return 'approved-native';
            if (command === 'prepare_native_tool' || command === 'execute_native_tool') {
              if (command === 'prepare_native_tool' && state.singleWindow)
                return { success: false, error: { code: 'NATIVE_APPROVAL_REQUIRED' }, data: {
                  pendingId: 'single-pending', preparedArguments: { ...args.arguments, targetWindowId: 'only-window' },
                  view: { question: '이 창을 닫을까요?', action: '닫기', targets: [{ label: '선택한 창', name: '메모장 · 단일 문서' }] },
                } };
              if (!args.arguments.targetWindowId)
                return {
                  success: false,
                  tool: args.tool,
                  error: { code: 'WINDOW_SELECTION_REQUIRED', message: '선택' },
                  data: {
                    choices: [1, 2, 3].map((n) => ({
                      targetId: 'window-' + n,
                      appName: '메모장',
                      title: '문서 ' + n,
                    })),
                  },
                };
              if (command === 'prepare_native_tool')
                return {
                  success: false,
                  error: { code: 'NATIVE_APPROVAL_REQUIRED' },
                  data: { pendingId: 'pending', description: '선택한 메모장 창을 종료할까요?' },
                };
              if (state.stale) {
                state.stale = false;
                return {
                  success: false,
                  tool: args.tool,
                  error: { code: 'WINDOW_TARGET_STALE', message: '만료' },
                  data: null,
                };
              }
              return {
                success: true,
                tool: args.tool,
                error: null,
                data: { closeRequested: true },
              };
            }
          },
        },
      });
    },
    { stale },
  );
  const conversation = {
    id: 'windows-test',
    title: '새 대화',
    isPinned: false,
    createdAt: '',
    updatedAt: '',
  };
  const messages: any[] = [];
  const results: any[] = [];
  await page.route('http://127.0.0.1:3002/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    const method = route.request().method();
    let data: any = { items: [], hasMore: false, nextCursor: null };
    if (path === '/users/me')
      data = { id: 'test', displayName: '테스트', email: 'test@example.test' };
    if (path === '/health') data = { ai: 'connected' };
    if (path === '/settings') data = { responseLanguage: 'ko', ttsEnabled: false };
    if (path === '/tools' || path === '/settings/permissions') data = [];
    if (path === '/conversations' && method === 'POST') data = conversation;
    if (path === '/conversations/windows-test') data = conversation;
    if (path === '/conversations/windows-test/messages') {
      if (method === 'POST') {
        data = {
          id: 'user-1',
          conversationId: conversation.id,
          role: 'USER',
          content: route.request().postDataJSON().content,
          attachments: [],
          createdAt: new Date().toISOString(),
        };
        messages.push(data);
      } else data = { items: messages.slice().reverse(), hasMore: false, nextCursor: null };
    }
    if (path === '/agent/turns')
      data = {
        conversationId: conversation.id,
        message: null,
        toolCalls: [
          {
            id: 'call-1',
            tool: 'app.close',
            arguments: { appName: '메모장' },
            executionLocation: 'LOCAL',
            requiresConfirmation: true,
            denied: false,
            ticket: 'test-ticket',
          },
        ],
      };
    if (path === '/agent/tool-results') {
      results.push(route.request().postDataJSON());
      data = { conversationId: conversation.id, message: null, toolCalls: [] };
    }
    await route.fulfill({ json: data });
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
  return { errors, results };
}

test('메인 창에서 창을 선택한 뒤에만 실행 결과를 제출', async ({ page }) => {
  const { errors, results } = await mock(page);
  await page.locator('textarea').fill('메모장 닫아줘');
  await page.getByRole('button', { name: '메시지 보내기', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '작업할 창 선택' });
  await expect(dialog).toBeVisible();
  expect(results).toHaveLength(0);
  await dialog.getByRole('button', { name: '메모장 · 문서 2', exact: true }).click();
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect(dialog).toBeHidden();
  await expect.poll(() => results.length).toBe(1);
  const calls = await page.evaluate(() =>
    (window as any).inputWindowTest.calls.filter((c: any) => c.command === 'execute_native_tool'),
  );
  expect(calls).toHaveLength(1);
  expect(calls[0].args.arguments.targetWindowId).toBe('window-2');
  expect(results[0].status).toBe('SUCCEEDED');
  expect(errors).toEqual([]);
});

test('단일 창은 선택 없이 Rust 대상 승인 후 같은 ID로 실행', async ({ page }) => {
  const { results } = await mock(page);
  await page.evaluate(() => { (window as any).inputWindowTest.singleWindow = true; });
  await page.locator('textarea').fill('메모장 닫아줘');
  await page.getByRole('button', { name: '메시지 보내기', exact: true }).click();
  await expect(page.getByText('메모장 · 단일 문서', { exact: true })).toBeVisible();
  await expect(page.getByRole('dialog', { name: '작업할 창 선택' })).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).inputWindowTest.calls.some((c: any) => c.command === 'execute_native_tool'))).toBe(false);
  await page.locator('.main-tool-confirmation').getByRole('button', { name: '닫기', exact: true }).click();
  await expect.poll(() => results.length).toBe(1);
  const calls = await page.evaluate(() => (window as any).inputWindowTest.calls);
  expect(calls.find((c: any) => c.command === 'execute_native_tool').args.arguments.targetWindowId).toBe('only-window');
  expect(calls.some((c: any) => c.command === 'authorize_native_tool')).toBe(true);
});
test('선택한 창이 사라지면 결과 제출 없이 다시 조회', async ({ page }) => {
  const { results, errors } = await mock(page, true);
  await page.locator('textarea').fill('메모장 닫아줘');
  await page.getByRole('button', { name: '메시지 보내기', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '작업할 창 선택' });
  await dialog.getByRole('button', { name: '메모장 · 문서 2', exact: true }).click();
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect(dialog.getByRole('button', { name: '다시 조회' })).toBeVisible();
  expect(results).toHaveLength(0);
  await dialog.getByRole('button', { name: '다시 조회' }).click();
  await dialog.getByRole('button', { name: '메모장 · 문서 3', exact: true }).click();
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect.poll(() => results.length).toBe(1);
  expect(errors).toEqual([]);
});

test('마이크 선택·자동 대체 설정·입력 음량·장치 해제 안내', async ({ page }) => {
  const { errors } = await mock(page);
  await page.getByRole('button', { name: '설정 열기' }).click();
  await page.getByLabel('입력 마이크', { exact: true }).selectOption('usb');
  await expect(
    page.locator('.setting-row p').filter({ hasText: '테스트 USB 마이크' }),
  ).toBeVisible();
  await page.getByLabel('기본 마이크로 자동 대체', { exact: true }).check();
  await page.getByRole('button', { name: '음량 확인', exact: true }).click();
  await page.evaluate(() => {
    const s = (window as any).inputWindowTest;
    s.callbacks[s.events['ace-audio-input']]({ payload: { sessionId: s.sessionId, rms: 0.075 } });
  });
  await expect(page.getByRole('meter', { name: '마이크 음량' })).toHaveAttribute('value', '0.5');
  await page.evaluate(() => {
    const s = (window as any).inputWindowTest;
    s.devices = s.devices.filter((d: any) => d.id !== 'usb');
    s.callbacks[s.events['ace-audio-input-error']]({
      payload: { sessionId: s.sessionId, message: '마이크 연결이 끊겼습니다. 다시 선택해 주세요.' },
    });
  });
  await expect(page.getByText('마이크 연결이 끊겼습니다. 다시 선택해 주세요.')).toBeVisible();
  await page.getByLabel('입력 마이크', { exact: true }).selectOption('builtin');
  await expect(page.getByLabel('입력 마이크', { exact: true })).toHaveValue('builtin');
  const preference = await page.evaluate(() => (window as any).inputWindowTest.settings);
  expect(preference).toEqual({ deviceId: 'builtin', allowDefaultFallback: true });
  expect(errors).toEqual([]);
});
