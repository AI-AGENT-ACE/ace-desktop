import { test, expect, type Page } from '@playwright/test';
async function setup(
  page: Page,
  tool = 'file.copy',
  intent = false,
  source = 'C:/test/source.txt',
  duplicateCopy = false,
) {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.addInitScript(
    ({ tool, source }) => {
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
        tool,
        calls: [] as any[],
        next: 1,
        callbacks: {} as any,
        events: {} as any,
        changed: false,
        overwriteFailure: '',
        closed: false,
        mode: 'region',
        captured: false,
        session: 'capture-test',
        cancelled: false,
        captureFailure: '',
        approvalFailure: '',
        holdSearch: false,
        releaseSearch: null as null | (() => void),
      };
      Object.assign(window, {
        isTauri: true,
        nativeSafety: state,
        __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
        __TAURI_INTERNALS__: {
          metadata: { currentWindow: { label: 'main' } },
          transformCallback: (fn: any) => {
            let id = state.next++;
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
            if (command === 'authorize_file_overwrite') return 'approved-once';
            if (command === 'authorize_native_tool') {
              if (state.approvalFailure) throw state.approvalFailure;
              return 'native-approved-once';
            }
            if (command === 'describe_native_target')
              return {
                question: '이 파일을 열까요?',
                action: '열기',
                targets: [
                  {
                    label: '대상',
                    name: 'Rust에서 확인한 파일.txt',
                    path: 'C:/test/Rust에서 확인한 파일.txt',
                  },
                ],
              };
            if (command === 'prepare_native_tool')
              return {
                success: false,
                error: { code: 'NATIVE_APPROVAL_REQUIRED' },
                data: {
                  pendingId: 'pending-native',
                  description: 'Rust에서 확인한 작업을 허용할까요?',
                  view: {
                    question:
                      args.tool === 'file.delete'
                        ? '이 파일을 휴지통으로 옮길까요?'
                        : args.tool === 'app.close'
                          ? '이 창을 닫을까요?'
                          : args.arguments.overwriteToken
                            ? '이 파일을 덮어쓸까요?'
                            : '이 파일을 복사할까요?',
                    action:
                      args.tool === 'file.delete'
                        ? '휴지통으로 이동'
                        : args.tool === 'app.close'
                          ? '닫기'
                          : args.arguments.overwriteToken
                            ? '덮어쓰기'
                            : '복사',
                    targets: [
                      { label: '원본', name: source.split(/[\\/]/).pop(), path: source },
                      { label: '대상', name: 'target.txt', path: 'C:/test/target.txt' },
                    ],
                    warning: args.arguments.overwriteToken
                      ? '대상 파일의 기존 내용이 바뀝니다.'
                      : '',
                  },
                },
              };
            if (command === 'save_capture_file') return 'Pictures/ACE Captures/saved.png';
            if (command === 'observe_window_close')
              return {
                closed: state.closed,
                message: state.closed
                  ? '선택한 창이 종료되었습니다.'
                  : '저장 확인 또는 사용자 조작이 필요합니다',
              };
            if (command === 'get_capture_context')
              return {
                sessionId: state.session,
                name: '테스트 모니터',
                mode: state.mode,
                width: 1600,
                height: 900,
                scaleFactor: 1.25,
              };
            if (command === 'complete_screen_capture') {
              state.captured = true;
              return;
            }
            if (command === 'cancel_screen_capture') {
              state.cancelled = true;
              return;
            }
            if (command === 'await_screen_capture') {
              if (state.captureFailure) throw state.captureFailure;
              return { resourceId: 'captured-image', displayName: 'capture-test.png' };
            }
            if (command === 'execute_native_tool') {
              if (args.tool === 'file.search' && state.holdSearch)
                await new Promise<void>((resolve) => { state.releaseSearch = resolve; });
              if (args.tool === 'clipboard.read')
                return { success: true, data: { text: 'ACE 읽기 테스트 123' } };
              if (args.tool === 'clipboard.write')
                return { success: true, data: { written: true } };
              if (args.tool === 'file.search')
                return {
                  success: true,
                  data: {
                    results: [
                      {
                        resourceId: 'source-choice',
                        displayName: '원본.txt',
                        relativePath: 'ACE테스트/원본.txt',
                      },
                    ],
                  },
                };
              if (args.tool === 'file.delete') return { success: true, data: { recycled: true } };
              if (args.tool === 'app.close')
                return {
                  success: true,
                  tool: args.tool,
                  data: { closeRequested: true, closeTicket: 'close-ticket' },
                };
              if (args.tool === 'screen.capture') {
                if (!args.arguments.monitorId)
                  return {
                    success: false,
                    error: { code: 'CAPTURE_SELECTION_REQUIRED', message: '선택' },
                    data: {
                      monitors: [
                        {
                          monitorId: 'monitor-a',
                          name: 'A',
                          width: 1920,
                          height: 1080,
                          scaleFactor: 1,
                        },
                        {
                          monitorId: 'monitor-b',
                          name: 'B',
                          width: 1600,
                          height: 900,
                          scaleFactor: 1.25,
                        },
                      ],
                    },
                  };
                return { success: true, data: { captureSessionId: state.session } };
              }
              if (args.tool === 'file.open') return { success: true, data: { opened: true } };
              if (args.arguments.overwriteToken && state.overwriteFailure)
                return {
                  success: false,
                  error: { code: state.overwriteFailure, message: '파일 작업에 실패했습니다.' },
                };
              if (
                (args.arguments.overwriteToken === 'approved-once' && !state.changed) ||
                args.arguments.destinationPath === 'other.txt'
              )
                return { success: true, data: { completed: true } };
              if (state.changed && args.arguments.overwriteToken) {
                state.changed = false;
                return {
                  success: false,
                  error: { code: 'FILE_CONFIRMATION_STALE', message: '파일이 변경되었습니다.' },
                };
              }
              return {
                success: false,
                error: { code: 'FILE_OVERWRITE_CONFIRMATION_REQUIRED', message: '충돌' },
                data: {
                  conflict: {
                    approvalId: 'confirmation-request',
                    source,
                    target: 'C:/test/target.txt',
                    sourceSize: 123,
                    targetSize: 456,
                    expiresInSeconds: 120,
                  },
                },
              };
            }
          },
        },
      });
    },
    { tool, source },
  );
  const conversation = {
    id: 'native-test',
    title: '새 대화',
    isPinned: false,
    createdAt: '',
    updatedAt: '',
  };
  const messages: any[] = [];
  const results: any[] = [];
  await page.route('http://127.0.0.1:3002/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    let data: any = { items: [], hasMore: false, nextCursor: null };
    if (path === '/users/me')
      data = { id: 'test', displayName: '테스트', email: 'test@example.test' };
    if (path === '/health') data = { ai: 'connected' };
    if (path === '/settings') data = { responseLanguage: 'ko', ttsEnabled: false };
    if (path === '/tools' || path === '/settings/permissions') data = [];
    if (path === '/tools')
      data = ['file.search', 'file.open', 'folder.open'].map((name) => ({
        name,
        defaultPolicy: 'ALWAYS_ALLOW',
        systemConfirmation: false,
      }));
    if (path === '/conversations' && route.request().method() === 'POST') data = conversation;
    if (path === '/conversations/native-test') data = conversation;
    if (path === '/conversations/native-test/messages') {
      if (route.request().method() === 'POST') {
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
            tool,
            arguments: intent
              ? {
                  query: {
                    original: '바탕화면 ACE테스트 폴더의 원본.txt',
                    candidates: ['원본.txt'],
                  },
                  extensions: [],
                  ...(tool === 'file.copy' && !duplicateCopy
                    ? { destination: { original: '대상.txt', candidates: ['대상.txt'] } }
                    : {}),
                }
              : tool === 'file.open'
                ? { resourceId: 'test-resource-id' }
                : tool === 'file.delete'
                  ? { query: { original: '대상.txt', candidates: ['대상.txt'] }, extensions: [] }
                  : tool === 'file.copy' || tool === 'file.move'
                    ? {
                        sourceDirectory: 'desktop',
                        sourcePath: 'source.txt',
                        destinationDirectory: 'desktop',
                        destinationPath: 'target.txt',
                      }
                    : tool === 'app.close'
                      ? { appName: '메모장' }
                      : tool === 'clipboard.write'
                        ? { text: 'ACE 새 텍스트' }
                        : {},
            executionLocation: 'LOCAL',
            requiresConfirmation: !['screen.capture', 'clipboard.read'].includes(tool),
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
  return { errors, results };
}
async function send(page: Page, confirm = true) {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
  await page.locator('textarea').fill('텍스트로 로컬 기능 시험');
  await page.getByRole('button', { name: '메시지 보내기', exact: true }).click();
  if (confirm)
    await page
      .locator('.main-tool-confirmation .approval-actions')
      .last()
      .getByRole('button')
      .last()
      .click();
}

test('목적지 없는 복사 의도는 원본 선택 후 목적지 없이 승인 준비', async ({ page }) => {
  await setup(page, 'file.copy', true, 'C:/test/source.txt', true);
  await send(page, false);
  await expect(
    page.getByRole('button', { name: 'ACE테스트/원본.txt', exact: true }),
  ).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByLabel('대상 파일 경로')).toHaveValue('');
  await page.getByRole('button', { name: '선택한 파일로 진행' }).click();
  await expect
    .poll(async () =>
      page.evaluate(
        () =>
          (window as any).nativeSafety.calls.filter(
            (c: any) => c.command === 'prepare_native_tool' && c.args.tool === 'file.copy',
          ).length,
      ),
    )
    .toBeGreaterThan(0);
  const args = await page.evaluate(
    () =>
      (window as any).nativeSafety.calls.find(
        (c: any) => c.command === 'prepare_native_tool' && c.args.tool === 'file.copy',
      ).args.arguments,
  );
  expect(args).toEqual({ resourceId: 'source-choice' });
});

test('채팅 검색 실행 중 찾는 중 표시 후 결과 수신 시 제거', async ({ page }) => {
  const { results } = await setup(page, 'file.search', true);
  await send(page, false);
  await page.evaluate(() => { (window as any).nativeSafety.holdSearch = true; });
  await page.locator('.main-tool-confirmation .approval-actions').last().getByRole('button').last().click();
  await expect(page.getByRole('status', { name: '찾는 중…' })).toBeVisible();
  await expect.poll(async () => page.evaluate(() => typeof (window as any).nativeSafety.releaseSearch)).toBe('function');
  await page.evaluate(() => (window as any).nativeSafety.releaseSearch());
  await expect.poll(() => results.length).toBe(1);
  await expect(page.getByRole('status', { name: '찾는 중…' })).toHaveCount(0);
  await expect(page.getByRole('dialog', { name: '파일 검색 결과' })).toBeVisible();
});

test('클립보드 읽기는 기본 승인 없이 실제 결과 텍스트를 API에 전달', async ({ page }) => {
  const { results, errors } = await setup(page, 'clipboard.read');
  await send(page, false);
  await expect.poll(() => results.length).toBe(1);
  expect(results[0].result.text).toBe('ACE 읽기 테스트 123');
  await expect(page.getByRole('textbox', { name: '읽은 클립보드 내용' })).toHaveCount(0);
  const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
  expect(
    calls.some((c: any) => c.command === 'execute_native_tool' && c.args.tool === 'clipboard.read'),
  ).toBe(true);
  expect(calls.some((c: any) => c.command === 'authorize_native_tool')).toBe(false);
  expect(errors).toEqual([]);
});

test('클립보드 승인 만료는 거절이 아닌 실패로 전달하고 실행하지 않음', async ({ page }) => {
  const { results } = await setup(page, 'clipboard.write');
  await send(page, false);
  await page.evaluate(() => {
    (window as any).nativeSafety.approvalFailure = 'NATIVE_APPROVAL_EXPIRED';
  });
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect.poll(() => results.length).toBe(1);
  expect(results[0].status).toBe('FAILED');
  expect(results[0].confirmed).toBe(false);
  expect(results[0].result.failureReason).toBe('NATIVE_APPROVAL_EXPIRED');
  expect(
    await page.evaluate(() =>
      (window as any).nativeSafety.calls.some((c: any) => c.command === 'execute_native_tool'),
    ),
  ).toBe(false);
});

test('클립보드 쓰기는 승인 전 실행하지 않고 승인 후 실행', async ({ page }) => {
  const { results, errors } = await setup(page, 'clipboard.write');
  await send(page, false);
  const actions = page.locator('.main-tool-confirmation .approval-actions').last();
  await expect(actions).toBeVisible();
  expect(
    await page.evaluate(() =>
      (window as any).nativeSafety.calls.some((c: any) => c.command === 'execute_native_tool'),
    ),
  ).toBe(false);
  await actions.getByRole('button').last().click();
  await expect.poll(() => results.length).toBe(1);
  const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
  expect(calls.some((c: any) => c.command === 'authorize_native_tool')).toBe(true);
  expect(calls.find((c: any) => c.command === 'execute_native_tool').args.arguments.text).toBe(
    'ACE 새 텍스트',
  );
  expect(results[0].result.text).toBeUndefined();
  expect(errors).toEqual([]);
});

test('파일 열기 승인에는 Rust가 확인한 대상 표시 후 열기 버튼 제공', async ({ page }) => {
  const { results } = await setup(page, 'file.open');
  await page.setViewportSize({ width: 600, height: 520 });
  await send(page, false);
  const dialog = page.getByRole('dialog', { name: '이 파일을 열까요?' });
  await expect(dialog.getByText('Rust에서 확인한 파일.txt', { exact: true })).toBeVisible();
  await expect(dialog.getByRole('button', { name: '열기', exact: true })).toBeEnabled();
  await page.screenshot({ path: 'test-results/approval-short-name-small.png' });
  await dialog.getByRole('button', { name: '열기', exact: true }).click();
  await expect.poll(() => results.length).toBe(1);
  const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
  expect(calls.find((c: any) => c.command === 'execute_native_tool').args.arguments).toEqual({
    resourceId: 'test-resource-id',
  });
});

test('긴 승인 대상과 전체 경로는 작은 창에서 펼치고 복사할 수 있음', async ({ page }) => {
  const source =
    'C:\\Users\\테스트 사용자\\Documents\\아주 깊은 폴더\\프로젝트 자료\\ACE-Native-Test\\' +
    '긴한글파일명_Long English filename '.repeat(4) +
    '.txt';
  await setup(page, 'file.copy', false, source);
  await page.setViewportSize({ width: 600, height: 520 });
  await send(page, false);
  const dialog = page.getByRole('dialog', { name: '이 파일을 복사할까요?' });
  await expect(dialog).toBeVisible();
  const original = dialog.locator('.approval-target').first();
  await expect(original.locator('.approval-target-name')).toHaveText(source.split('\\').pop()!);
  await original.getByText('전체 경로 보기', { exact: true }).focus();
  await page.keyboard.press('Enter');
  await expect(original.locator('.approval-full-path')).toHaveText(source);
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await original.getByRole('button', { name: '경로 복사' }).click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(source);
  expect(await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
  const bounds = await dialog.boundingBox();
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(600);
  expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(520);
  await page.screenshot({ path: 'test-results/approval-long-path-small.png' });
  await dialog
    .locator('.approval-actions')
    .getByRole('button', { name: '복사', exact: true })
    .scrollIntoViewIfNeeded();
  await dialog
    .locator('.approval-actions')
    .getByRole('button', { name: '복사', exact: true })
    .click();
  const conflict = page.getByRole('dialog', { name: '파일 덮어쓰기 확인' });
  await expect(conflict).toBeVisible();
  await expect(
    conflict.getByText('대상 파일의 기존 내용이 바뀝니다.', { exact: true }),
  ).toBeVisible();
  await page.screenshot({ path: 'test-results/conflict-long-path-small.png' });
  const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
  expect(
    calls.find((c: any) => c.command === 'execute_native_tool').args.arguments.sourcePath,
  ).toBe('source.txt');
});

test('AI 파일 검색 의도는 사용자 원본·대상 선택 뒤에만 복사와 덮어쓰기로 연결', async ({
  page,
}) => {
  const { results, errors } = await setup(page, 'file.copy', true);
  await send(page, false);
  await expect(page.getByRole('dialog', { name: '파일 대상 확인' })).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as any).nativeSafety.calls.filter(
          (c: any) => c.command === 'execute_native_tool' && c.args.tool === 'file.copy',
        ).length,
    ),
  ).toBe(0);
  await expect(
    page.getByRole('button', { name: 'ACE테스트/원본.txt', exact: true }),
  ).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByLabel('대상 파일 경로')).toHaveValue('ACE테스트/대상.txt');
  const searches = await page.evaluate(() =>
    (window as any).nativeSafety.calls.filter(
      (c: any) => c.command === 'execute_native_tool' && c.args.tool === 'file.search',
    ),
  );
  expect(searches.at(-1).args.arguments).toMatchObject({
    directory: 'desktop',
    query: 'ACE테스트/원본.txt',
  });
  await page.getByRole('button', { name: '선택한 파일로 진행', exact: true }).click();
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect(page.getByRole('dialog', { name: '파일 덮어쓰기 확인' })).toBeVisible();
  expect(results).toHaveLength(0);
  await page.getByRole('button', { name: '덮어쓰기', exact: true }).click();
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect.poll(() => results.length).toBe(1);
  expect(results[0].status).toBe('SUCCEEDED');
  expect(errors).toEqual([]);
});
test('AI 파일 삭제 의도는 선택한 resourceId로 휴지통 명령 실행', async ({ page }) => {
  const { results, errors } = await setup(page, 'file.delete');
  await send(page, false);
  await page.getByRole('button', { name: '파일 찾기', exact: true }).click();
  await page.getByRole('button', { name: 'ACE테스트/원본.txt', exact: true }).click();
  await page.getByRole('button', { name: '선택한 파일로 진행', exact: true }).click();
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect.poll(() => results.length).toBe(1);
  const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
  expect(
    calls.find((c: any) => c.command === 'execute_native_tool' && c.args.tool === 'file.delete')
      .args,
  ).toMatchObject({
    arguments: { resourceId: 'source-choice' },
    approvalId: 'native-approved-once',
  });
  expect(errors).toEqual([]);
});
test('덮어쓰기 선택 후에만 별도 승인 토큰을 사용하고 결과 제출', async ({ page }) => {
  const { errors, results } = await setup(page);
  await send(page);
  const dialog = page.getByRole('dialog', { name: '파일 덮어쓰기 확인' });
  await expect(dialog).toContainText('C:/test/source.txt');
  await expect(dialog).toContainText('C:/test/target.txt');
  expect(results).toHaveLength(0);
  await dialog.getByRole('button', { name: '덮어쓰기', exact: true }).click();
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect.poll(() => results.length).toBe(1);
  const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
  expect(calls.findIndex((c: any) => c.command === 'authorize_file_overwrite')).toBeGreaterThan(
    calls.findIndex((c: any) => c.command === 'execute_native_tool'),
  );
  expect(
    calls.filter((c: any) => c.command === 'execute_native_tool')[1].args.arguments.overwriteToken,
  ).toBe('approved-once');
  expect(errors).toEqual([]);
});
for (const tool of ['file.copy', 'file.move']) {
  test(`${tool} 덮어쓰기 실패는 성공 제출 없이 대기를 해제`, async ({ page }) => {
    const { results, errors } = await setup(page, tool);
    await send(page);
    await page.evaluate(() => {
      (window as any).nativeSafety.overwriteFailure = 'EXECUTION_FAILED';
    });
    await page
      .getByRole('dialog', { name: '파일 덮어쓰기 확인' })
      .getByRole('button', { name: '덮어쓰기', exact: true })
      .click();
    await page
      .locator('.main-tool-confirmation .approval-actions')
      .last()
      .getByRole('button')
      .last()
      .click();
    await expect.poll(() => results.length).toBe(1);
    expect(JSON.stringify(results[0])).toContain('FAILED');
    expect(JSON.stringify(results[0])).not.toContain('SUCCEEDED');
    await expect(page.getByRole('dialog', { name: '파일 덮어쓰기 확인' })).toHaveCount(0);
    await page.locator('textarea').fill('실패 후 새 요청');
    await expect(page.getByRole('button', { name: '메시지 보내기', exact: true })).toBeEnabled();
    expect(errors).toEqual([]);
  });
}

test('다른 이름 지정은 기존 대상 승인 토큰을 폐기하고 새 경로 사용', async ({ page }) => {
  const { results, errors } = await setup(page);
  await send(page);
  const dialog = page.getByRole('dialog', { name: '파일 덮어쓰기 확인' });
  await dialog.getByLabel('다른 파일 이름').fill('other.txt');
  await dialog.getByRole('button', { name: /다른 이름으로 (복사|이동)/ }).click();
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect.poll(() => results.length).toBe(1);
  const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
  expect(calls.some((c: any) => c.command === 'discard_file_approval')).toBe(true);
  const last = calls.filter((c: any) => c.command === 'execute_native_tool').at(-1);
  expect(last.args.arguments.destinationPath).toBe('other.txt');
  expect(last.args.arguments.overwriteToken).toBeUndefined();
  expect(errors).toEqual([]);
});
for (const tool of ['file.copy', 'file.move']) {
  test(`${tool} 다른 이름의 확장자 자동 보완 후 최종 경로로 승인`, async ({ page }) => {
    const { results } = await setup(page, tool);
    await send(page);
    const dialog = page.getByRole('dialog', { name: '파일 덮어쓰기 확인' });
    await dialog.getByLabel('다른 파일 이름').fill('other');
    await expect(dialog.locator('#conflict-final-name')).toContainText('other.txt');
    await dialog.getByRole('button', { name: /다른 이름으로 (복사|이동)/ }).click();
    await page
      .locator('.main-tool-confirmation .approval-actions')
      .last()
      .getByRole('button')
      .last()
      .click();
    await expect.poll(() => results.length).toBe(1);
    const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
    const last = calls.filter((c: any) => c.command === 'execute_native_tool').at(-1);
    expect(last.args.arguments.destinationPath).toBe('other.txt');
    expect(last.args.arguments.overwriteToken).toBeUndefined();
    expect(
      calls.filter((c: any) => c.command === 'prepare_native_tool').at(-1).args.arguments
        .destinationPath,
    ).toBe('other.txt');
  });
}
test('확장자 변경은 명시적 확인이 필요하고 이름을 수정하면 확인 해제', async ({ page }) => {
  await setup(page);
  await send(page);
  const dialog = page.getByRole('dialog', { name: '파일 덮어쓰기 확인' });
  const input = dialog.getByLabel('다른 파일 이름');
  const button = dialog.getByRole('button', { name: /다른 이름으로 (복사|이동)/ });
  await input.fill('other.csv');
  await expect(dialog.getByText(/파일 내용이나 형식이 변환되지는 않습니다/)).toBeVisible();
  await expect(button).toBeDisabled();
  await dialog.getByLabel('확장자 변경 확인').check();
  await expect(button).toBeEnabled();
  await input.fill('changed.csv');
  await expect(button).toBeDisabled();
  await input.fill('other.TXT');
  await expect(button).toBeEnabled();
  await expect(dialog.locator('#conflict-final-name')).toContainText('other.TXT');
});
test('확장자 없는 원본은 다른 이름에 확장자를 추가하지 않음', async ({ page }) => {
  await setup(page, 'file.copy', false, 'C:/test/README');
  await send(page);
  const dialog = page.getByRole('dialog', { name: '파일 덮어쓰기 확인' });
  await dialog.getByLabel('다른 파일 이름').fill('복사본');
  await expect(dialog.locator('#conflict-final-name strong')).toHaveText('복사본');
  await expect(dialog.getByRole('button', { name: /다른 이름으로 (복사|이동)/ })).toBeEnabled();
});

test('변경된 파일은 재확인 UI를 유지하고 자동으로 덮어쓰지 않음', async ({ page }) => {
  const { results, errors } = await setup(page);
  await send(page);
  await page.evaluate(() => {
    (window as any).nativeSafety.changed = true;
  });
  const dialog = page.getByRole('dialog', { name: '파일 덮어쓰기 확인' });
  await dialog.getByRole('button', { name: '덮어쓰기', exact: true }).click();
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect(dialog.getByRole('button', { name: '다시 확인' })).toBeVisible();
  await expect(dialog.getByRole('button', { name: '덮어쓰기', exact: true })).toBeDisabled();
  expect(results).toHaveLength(0);
  await dialog.getByRole('button', { name: '취소', exact: true }).click();
  await expect.poll(() => results.length).toBe(1);
  expect(results[0].status).toBe('DENIED');
  expect(errors).toEqual([]);
});
test('닫히지 않은 창은 종료 완료 대신 사용자 조작 안내', async ({ page }) => {
  const { results, errors } = await setup(page, 'app.close');
  await send(page);
  await expect(
    page.getByText('저장 확인 또는 사용자 조작이 필요합니다', { exact: true }),
  ).toBeVisible();
  await expect.poll(() => results.length).toBe(1);
  expect(results[0].result.summary).toContain('사용자 조작');
  expect(results[0].result.summary).not.toContain('종료된 것을 확인');
  expect(errors).toEqual([]);
});
test('실제 창 종료를 확인한 경우에만 종료 확인 결과 전달', async ({ page }) => {
  const { results, errors } = await setup(page, 'app.close');
  await page.goto('/');
  await page.evaluate(() => {
    (window as any).nativeSafety.closed = true;
  });
  await page.locator('textarea').fill('메모장 종료');
  await page.getByRole('button', { name: '메시지 보내기', exact: true }).click();
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect.poll(() => results.length).toBe(1);
  expect(results[0].result.summary).toBe('선택한 창이 종료된 것을 확인함');
  expect(errors).toEqual([]);
});
test('모니터 선택 뒤 캡처 완료를 기다리고 이미지 열기 제공', async ({ page }) => {
  const { results, errors } = await setup(page, 'screen.capture');
  await page.setViewportSize({ width: 600, height: 520 });
  await send(page, false);
  await expect(page.getByRole('dialog', { name: '캡처 모니터 선택' })).toBeVisible();
  await page.screenshot({ path: 'test-results/capture-monitor-small.png' });
  await page.getByRole('button', { name: '모니터 2 전체 캡처' }).click();
  await expect(page.getByText('화면 캡처 완료', { exact: true })).toBeVisible();
  await expect.poll(() => results.length).toBe(1);
  await page.screenshot({ path: 'test-results/capture-result-small.png' });
  expect(
    await page.locator('.capture-result').evaluate((el) => el.scrollWidth <= el.clientWidth),
  ).toBe(true);
  await page.getByRole('button', { name: '이미지 열기' }).click();
  const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
  expect(calls.filter((c: any) => c.command === 'execute_native_tool')[1].args.arguments).toEqual({
    monitorId: 'monitor-b',
    mode: 'full',
  });
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).nativeSafety.calls.some(
          (call: any) => call.command === 'execute_native_tool' && call.args.tool === 'file.open',
        ),
      ),
    )
    .toBe(true);
  await page.getByLabel('캡처 저장 파일 이름').fill('saved.png');
  await page.getByRole('button', { name: '다른 이름으로 저장', exact: true }).click();
  await page.getByRole('button', { name: '캡처 결과 닫기' }).click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).nativeSafety.calls.some(
          (call: any) =>
            call.command === 'release_capture_file' && call.args.resourceId === 'captured-image',
        ),
      ),
    )
    .toBe(true);
  expect(errors).toEqual([]);
});
for (const [code, message] of [
  ['CAPTURE_CANCELLED', '화면 캡처를 취소했습니다.'],
  ['SCREEN_CAPTURE_FAILED', '화면 캡처에 실패했습니다.'],
  ['CAPTURE_MONITOR_DISCONNECTED', '선택한 모니터의 연결 또는 배치가 변경되었습니다.'],
]) {
  test(`캡처 ${code} 결과 후 대기 해제와 이유 표시`, async ({ page }) => {
    const { results } = await setup(page, 'screen.capture');
    await send(page, false);
    await page.evaluate((code) => {
      (window as any).nativeSafety.captureFailure = code;
    }, code);
    await page.getByRole('button', { name: '모니터 2 전체 캡처' }).click();
    await expect(page.getByText(message, { exact: true }).first()).toBeVisible();
    await expect.poll(() => results.length).toBe(1);
    await expect(page.getByText('화면 캡처 완료', { exact: true })).toHaveCount(0);
    await expect(page.getByRole('button', { name: '모니터 2 전체 캡처' })).toHaveCount(0);
  });
}

test('영역 선택 좌표와 Esc 취소는 캡처 전용 창에서 처리', async ({ page }) => {
  const { errors } = await setup(page);
  await page.setViewportSize({ width: 600, height: 520 });
  await page.goto('/#capture?session=capture-test');
  await page.getByRole('button', { name: '취소', exact: true }).waitFor();
  await page.screenshot({ path: 'test-results/capture-selector.png' });
  await expect(page.getByRole('dialog', { name: '화면 캡처 선택' })).toBeVisible();
  await page.mouse.move(100, 180);
  await page.mouse.down();
  await page.mouse.move(260, 280);
  await page.mouse.up();
  await page.getByRole('button', { name: '캡처', exact: true }).click();
  const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
  expect(calls.find((c: any) => c.command === 'complete_screen_capture').args.region).toEqual({
    x: 100,
    y: 180,
    width: 160,
    height: 100,
  });
  expect(calls.some((c: any) => c.command === 'plugin:event|listen')).toBe(false);
  await page.reload();
  await expect(page.getByRole('dialog', { name: '화면 캡처 선택' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect.poll(() => page.evaluate(() => (window as any).nativeSafety.cancelled)).toBe(true);
  expect(errors).toEqual([]);
});

test('전체 캡처는 추가 버튼 클릭 없이 한 번만 완료 요청', async ({ page }) => {
  const { errors } = await setup(page);
  await page.addInitScript(() => {
    (window as any).nativeSafety.mode = 'full';
  });
  await page.goto('/#capture?session=capture-test');
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).nativeSafety.calls.filter(
            (c: any) => c.command === 'complete_screen_capture',
          ).length,
      ),
    )
    .toBe(1);
  const call = await page.evaluate(() =>
    (window as any).nativeSafety.calls.find((c: any) => c.command === 'complete_screen_capture'),
  );
  expect(call.args).toEqual({ sessionId: 'capture-test', region: null });
  expect(errors).toEqual([]);
});

test('AI 파일 열기 검색 의도는 실제 검색 결과 선택을 거쳐 resourceId로 실행', async ({ page }) => {
  const { results, errors } = await setup(page, 'file.open', true);
  await send(page, false);
  await page.getByRole('button', { name: '파일 찾기', exact: true }).click();
  await page.getByRole('button', { name: 'ACE테스트/원본.txt', exact: true }).click();
  await page.getByRole('button', { name: '선택한 파일로 진행', exact: true }).click();
  const approval = page.locator('.main-tool-confirmation .approval-actions').last();
  if (await approval.count()) await approval.getByRole('button').last().click();
  await expect.poll(() => results.length).toBe(1);
  const calls = await page.evaluate(() => (window as any).nativeSafety.calls);
  const opened = calls.find(
    (c: any) => c.command === 'execute_native_tool' && c.args.tool === 'file.open',
  );
  expect(opened.args.arguments).toEqual({ resourceId: 'source-choice' });
  expect(errors).toEqual([]);
});

test('위치 없는 파일 검색은 ASK 유지, 단일 결과창 및 비민감 결과 요약 전달', async ({ page }) => {
  const { results, errors } = await setup(page, 'file.search', true);
  await send(page, false);
  const scope = page.getByRole('dialog', { name: '파일 검색 위치 선택' });
  await expect(scope).toHaveCount(0);
  expect(
    await page.evaluate(
      () =>
        (window as any).nativeSafety.calls.filter((c: any) => c.command === 'execute_native_tool')
          .length,
    ),
  ).toBe(0);
  await page
    .locator('.main-tool-confirmation .approval-actions')
    .last()
    .getByRole('button')
    .last()
    .click();
  await expect.poll(() => results.length).toBe(1);
  await expect(page.getByRole('dialog', { name: '파일 검색 결과' })).toContainText(
    'ACE테스트/원본.txt',
  );
  const call = await page.evaluate(() =>
    (window as any).nativeSafety.calls.find(
      (c: any) => c.command === 'execute_native_tool' && c.args.tool === 'file.search',
    ),
  );
  expect(call.args.arguments.directory).toBeUndefined();
  expect(results[0].result.search).toEqual({ count: 1, partial: false, displayedLocally: true });
  expect(JSON.stringify(results[0].result)).not.toContain('원본.txt');
  await expect(page.getByRole('dialog', { name: '검색 결과', exact: true })).toHaveCount(0);
  await page
    .getByRole('dialog', { name: '파일 검색 결과', exact: true })
    .getByRole('button', { name: '닫기', exact: true })
    .click();
  await expect(page.getByRole('dialog', { name: '파일 검색 결과', exact: true })).toHaveCount(0);
  expect(errors).toEqual([]);
});
