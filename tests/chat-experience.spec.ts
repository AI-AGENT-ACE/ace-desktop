import { test, expect } from '@playwright/test';

test('보낸 메시지는 즉시 보이고 취소 후 남으며 다음 응답은 타이핑 후 완료된다', async ({
  page,
}) => {
  const conversation = {
    id: 'chat-test',
    title: '새 대화',
    isPinned: false,
    createdAt: '',
    updatedAt: '',
  };
  const messages: Record<string, unknown>[] = [];
  let turns = 0;
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.addInitScript(() =>
    sessionStorage.setItem(
      'ace-auth-session',
      JSON.stringify({
        accessToken: 'test',
        refreshToken: 'test',
        tokenType: 'Bearer',
        expiresIn: 900,
        refreshExpiresAt: '',
      }),
    ),
  );
  await page.route('http://127.0.0.1:3002/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    let data: unknown = { items: [], hasMore: false, nextCursor: null };
    if (path === '/users/me')
      data = { id: 'test', displayName: '테스트', email: 'test@example.test' };
    if (path === '/health') data = { ai: 'connected' };
    if (path === '/settings') data = { responseLanguage: 'ko', ttsEnabled: false };
    if (path === '/tools' || path === '/settings/permissions') data = [];
    if (path === '/conversations' && route.request().method() === 'POST') data = conversation;
    if (path === '/conversations/chat-test') data = conversation;
    if (path === '/conversations/chat-test/messages') {
      if (route.request().method() === 'POST') {
        const content = route.request().postDataJSON().content;
        data = {
          id: 'user-' + messages.length,
          conversationId: conversation.id,
          role: 'USER',
          content,
          attachments: [],
          createdAt: new Date().toISOString(),
        };
        messages.push(data as Record<string, unknown>);
        if (conversation.title === '새 대화') conversation.title = content;
      } else data = { items: messages.slice().reverse(), hasMore: false, nextCursor: null };
    }
    if (path === '/agent/turns') {
      turns++;
      expect(route.request().postDataJSON().messageId).toMatch(/^user-/);
      if (turns === 1) {
        await held;
        data = {
          conversationId: conversation.id,
          message: { id: 'late', role: 'ASSISTANT', content: '취소된 답변' },
          toolCalls: [],
        };
      } else {
        const message = {
          id: 'answer',
          conversationId: conversation.id,
          role: 'ASSISTANT',
          content: '다음 요청에 대한 답변입니다. '.repeat(120),
          attachments: [],
          createdAt: new Date().toISOString(),
        };
        messages.push(message);
        messages.push({
          id: 'tool-log',
          conversationId: conversation.id,
          role: 'TOOL',
          content: 'app.open: SUCCEEDED',
          attachments: [],
          createdAt: new Date().toISOString(),
        });
        data = { conversationId: conversation.id, message, toolCalls: [] };
      }
    }
    await route.fulfill({ json: data });
  });
  await page.goto('/');
  const input = page.getByRole('textbox', { name: '메시지', exact: true });
  await input.fill('첫 번째 요청');
  await page.getByRole('button', { name: '메시지 보내기', exact: true }).click();
  await expect(input).toHaveValue('');
  await expect(input).toBeEnabled();
  await input.fill('답변을 기다리는 동안 작성');
  await expect(page.getByRole('button', { name: '메시지 보내기', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '답변 생성 중지' })).toBeEnabled();
  await expect(page.locator('.message.user .markdown')).toHaveText('첫 번째 요청');
  await expect(page.getByText('답변을 준비하고 있어요. 원하면 언제든 멈출 수 있어요.')).toHaveCount(
    0,
  );
  await expect(page.getByRole('button', { name: '최신 메시지로 이동' })).toHaveCount(0);
  await expect(page.locator('.thinking-message')).toBeVisible();
  await expect.poll(() => turns).toBe(1);
  await page.screenshot({ path: 'artifacts/chat-thinking.png' });
  await page.getByRole('button', { name: '답변 생성 중지' }).click();
  await expect(page.locator('.thinking-message')).toHaveCount(0);
  await expect(input).toBeEnabled();
  release();
  await expect(page.locator('.message.user')).toContainText('첫 번째 요청');
  await expect(page.getByText('취소된 답변')).toHaveCount(0);
  await input.fill('두 번째 요청');
  await page.getByRole('button', { name: '메시지 보내기', exact: true }).click();
  await expect(page.locator('.typing-cursor')).toBeVisible();
  await expect(input).toBeEnabled();
  await input.fill('답변을 마친 뒤 보낼 내용');
  await expect(page.getByRole('button', { name: '메시지 보내기', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: '응답 복사' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '응답 복사' })).toBeVisible();
  await expect(page.getByRole('button', { name: '메시지 보내기', exact: true })).toBeEnabled();
  await expect(page.locator('.message.user')).toHaveCount(2);
  await expect(page.getByText('app.open: SUCCEEDED')).toHaveCount(0);
  await expect(page.locator('.response-divider')).toHaveText('|');
  await expect(page.getByRole('button', { name: '최신 메시지로 이동' })).toHaveCount(0);
  await page.locator('.message-area').evaluate((element) => {
    element.scrollTop = 120;
  });
  await expect(page.getByRole('button', { name: '최신 메시지로 이동' })).toBeVisible();
  const bubble = page.locator('.message.user').last();
  await bubble.scrollIntoViewIfNeeded();
  await page.mouse.move(5, 5);
  await expect(bubble.locator('.user-copy-actions')).toHaveCSS('opacity', '0');
  const before = await bubble.boundingBox();
  const following = await page.locator('.message.assistant').boundingBox();
  await bubble.hover();
  await expect(bubble.locator('.user-copy-actions')).toHaveCSS('opacity', '1');
  expect(await bubble.boundingBox()).toEqual(before);
  expect(await page.locator('.message.assistant').boundingBox()).toEqual(following);
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await bubble.getByRole('button', { name: '사용자 메시지 복사' }).click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('두 번째 요청');
  await page.getByRole('button', { name: '최신 메시지로 이동' }).click();
  await expect(page.getByRole('button', { name: '최신 메시지로 이동' })).toHaveCount(0);
  await page.screenshot({ path: 'artifacts/chat-complete.png' });
  await page.getByRole('button', { name: '설정 열기', exact: true }).click();
  await expect(page.getByRole('button', { name: '내 목소리로 ACE 호출어 녹음' })).toBeVisible();
  await expect(page.getByText('녹음은 선택 사항입니다.', { exact: false })).toBeVisible();
  await expect(page.getByText('개발용 Wake Word 진단 녹음')).toHaveCount(0);
  await expect(page.getByText('계정 데이터는 서버에 저장됩니다.')).toHaveCount(0);
  await expect(page.locator('.settings-footnote')).toContainText('ACE 0.1.0');
  await page.screenshot({ path: 'artifacts/settings-simplified.png' });
});

test('메인 채팅 승인은 오른쪽 위에 표시하며 성공 배너와 도구 원문을 숨긴다', async ({ page }) => {
  const conversation = {
    id: 'confirmation-chat',
    title: '메모장 열어줘',
    isPinned: false,
    createdAt: '',
    updatedAt: '',
  };
  const items: Record<string, unknown>[] = [];
  let executions = 0;
  await page.addInitScript(() =>
    sessionStorage.setItem(
      'ace-auth-session',
      JSON.stringify({
        accessToken: 'test',
        refreshToken: 'test',
        tokenType: 'Bearer',
        expiresIn: 900,
        refreshExpiresAt: '',
      }),
    ),
  );
  await page.route('http://127.0.0.1:3002/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    let data: unknown = { items: [], hasMore: false, nextCursor: null };
    if (path === '/users/me')
      data = { id: 'test', displayName: '테스트', email: 'test@example.test' };
    if (path === '/health') data = { ai: 'connected' };
    if (path === '/settings') data = { ttsEnabled: false, responseLanguage: 'ko' };
    if (
      (path === '/conversations' && route.request().method() === 'POST') ||
      path === '/conversations/confirmation-chat'
    )
      data = conversation;
    if (path.endsWith('/messages')) {
      if (route.request().method() === 'POST') {
        data = {
          id: 'saved',
          conversationId: conversation.id,
          role: 'USER',
          content: '메모장 열어줘',
          attachments: [],
          createdAt: '',
        };
        items.push(data as Record<string, unknown>);
      } else data = { items: items.slice().reverse(), hasMore: false, nextCursor: null };
    }
    if (path === '/agent/turns')
      data = {
        conversationId: conversation.id,
        message: null,
        toolCalls: [
          {
            id: 'tool',
            tool: 'app.open',
            arguments: { appName: '메모장' },
            executionLocation: 'CLOUD',
            requiresConfirmation: true,
            denied: false,
            ticket: 'test',
          },
        ],
      };
    if (path === '/agent/cloud-tools') {
      executions++;
      items.push({
        id: 'log',
        conversationId: conversation.id,
        role: 'TOOL',
        content: 'app.open: SUCCEEDED',
        attachments: [],
        createdAt: '',
      });
      data = { conversationId: conversation.id, message: null, toolCalls: [] };
    }
    await route.fulfill({ json: data });
  });
  await page.goto('/');
  await page.getByRole('textbox', { name: '메시지', exact: true }).fill('메모장 열어줘');
  await page.getByRole('button', { name: '메시지 보내기', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '메모장을 실행할까요?' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('button', { name: '허용', exact: true })).toBeVisible();
  await expect(dialog.getByRole('button', { name: '취소', exact: true })).toBeFocused();
  const box = await dialog.boundingBox();
  expect(box!.y).toBe(52);
  expect(box!.x).toBeGreaterThan(page.viewportSize()!.width / 2);
  expect(executions).toBe(0);
  await page.screenshot({ path: 'artifacts/main-tool-confirmation.png' });
  await dialog.getByRole('button', { name: '허용', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(executions).toBe(1);
  await expect(page.locator('.system-action')).toHaveCount(0);
  await expect(page.getByText('app.open: SUCCEEDED')).toHaveCount(0);
});
