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
          content: '다음 요청에 대한 답변입니다. '.repeat(12),
          attachments: [],
          createdAt: new Date().toISOString(),
        };
        messages.push(message);
        data = { conversationId: conversation.id, message, toolCalls: [] };
      }
    }
    await route.fulfill({ json: data });
  });
  await page.goto('/');
  const input = page.getByRole('textbox', { name: '메시지', exact: true });
  await input.fill('첫 번째 요청');
  await page.getByRole('button', { name: '메시지 보내기', exact: true }).click();
  await expect(page.locator('.message.user')).toHaveText('나첫 번째 요청');
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
  await expect(page.getByRole('button', { name: '응답 복사' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '응답 복사' })).toBeVisible();
  await expect(page.locator('.message.user')).toHaveCount(2);
  await page.screenshot({ path: 'artifacts/chat-complete.png' });
  await page.getByRole('button', { name: '설정 열기', exact: true }).click();
  await expect(page.getByRole('button', { name: '내 목소리로 ACE 호출어 녹음' })).toBeVisible();
  await expect(page.getByText('녹음은 선택 사항입니다.', { exact: false })).toBeVisible();
  await expect(page.getByText('개발용 Wake Word 진단 녹음')).toHaveCount(0);
  await expect(page.getByText('계정 데이터는 서버에 저장됩니다.')).toHaveCount(0);
  await expect(page.locator('.settings-footnote')).toContainText('ACE 0.1.0');
  await page.screenshot({ path: 'artifacts/settings-simplified.png' });
});
