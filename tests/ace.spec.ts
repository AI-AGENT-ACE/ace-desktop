import { test, expect } from '@playwright/test';
import { login } from './helpers';

test('실제 API: 대화와 메시지 저장, 이름 변경, 고정, 휴지통 복구와 영구 삭제', async ({ page }) => {
  await login(page);
  await page.getByRole('button', { name: '새 채팅 새로운 시작' }).click();
  await page.getByRole('textbox', { name: '메시지', exact: true }).fill('실제 DB 메시지 검증');
  await page.getByRole('button', { name: '메시지 보내기' }).click();
  await expect(page.locator('.message.user')).toContainText('실제 DB 메시지 검증');
  await expect(page.locator('.markdown table')).toBeVisible();
  await page.getByRole('button', { name: '새 대화 메뉴' }).click();
  await page.getByRole('button', { name: '이름 변경', exact: true }).click();
  await page.getByRole('textbox', { name: '대화 이름' }).fill('검증용 대화');
  await page.getByRole('button', { name: '저장', exact: true }).click();
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await page.getByRole('button', { name: '검증용 대화 메뉴' }).click();
  await page.getByRole('button', { name: '고정', exact: true }).click();
  await expect(page.locator('.conversation-select svg')).toHaveClass(/lucide-pin/);
  await page.reload();
  await page.getByRole('button', { name: '검증용 대화', exact: true }).click();
  await expect(page.locator('.message.user')).toContainText('실제 DB 메시지 검증');
  await page.getByRole('button', { name: '검증용 대화 메뉴' }).click();
  await page.getByRole('button', { name: '삭제', exact: true }).click();
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
  await page.getByRole('button', { name: '휴지통', exact: true }).click();
  await page.getByRole('button', { name: '검증용 대화 복구' }).click();
  await expect(page.getByRole('button', { name: '검증용 대화 복구' })).not.toBeVisible();
  await page.getByRole('button', { name: '닫기', exact: true }).click();
  await page.getByRole('button', { name: '검증용 대화 메뉴' }).click();
  await page.getByRole('button', { name: '삭제', exact: true }).click();
  await expect(page.getByRole('button', { name: '검증용 대화 메뉴' })).not.toBeVisible();
  await page.getByRole('button', { name: '휴지통', exact: true }).click();
  await page.getByRole('button', { name: '검증용 대화 영구 삭제' }).click();
  await page.getByRole('button', { name: '영구 삭제', exact: true }).click();
  await expect(page.getByRole('heading', { name: '휴지통이 비어 있습니다' })).toBeVisible();
});

test('20개 대화와 30개 메시지 페이지 누적, 중복 요청 방지와 스크롤 위치 유지', async ({ page }) => {
  const cursors: string[] = [];
  page.on('request', (request) => {
    const url = new URL(request.url());
    if (url.pathname === '/conversations' && url.searchParams.has('cursor'))
      cursors.push(url.searchParams.get('cursor')!);
  });
  await login(page, true);
  await expect(page.locator('.conversation')).toHaveCount(20);
  await page.locator('.conversation-list').evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(page.locator('.conversation')).toHaveCount(25);
  expect(new Set(cursors).size).toBe(cursors.length);
  await page.getByRole('textbox', { name: '대화 검색' }).fill('페이지 메시지');
  await expect(page.locator('.conversation')).toHaveCount(1);
  await page.getByRole('button', { name: '페이지 메시지', exact: true }).click();
  await expect(page.locator('.message')).toHaveCount(30);
  await page.locator('.message-area').evaluate((element) => {
    element.scrollTop = 0;
  });
  await expect(page.locator('.message')).toHaveCount(60);
  expect(
    await page.locator('.message-area').evaluate((element) => element.scrollTop),
  ).toBeGreaterThan(100);
  await page.locator('.message-area').evaluate((element) => {
    element.scrollTop = 0;
  });
  await expect(page.locator('.message')).toHaveCount(68);
  const input = page.getByRole('textbox', { name: '메시지', exact: true });
  await input.fill('첫 줄');
  await input.press('Shift+Enter');
  await input.press('a');
  await expect(input).toHaveValue('첫 줄\na');
  await page.getByRole('button', { name: '사이드바 접기' }).click();
  await expect(page.locator('.sidebar')).not.toBeVisible();
  await page.getByRole('button', { name: '사이드바 펼치기' }).click();
  await expect(page.locator('.sidebar')).toBeVisible();
});

test('AI 미연결 상태에서도 실제 메시지 API로 저장하고 중복 전송을 막음', async ({ page }) => {
  await page.route('**/health', (route) =>
    route.fulfill({
      json: {
        status: 'ok',
        database: 'connected',
        ai: 'not_configured',
        weather: 'not_configured',
      },
    }),
  );
  await login(page);
  await page.getByRole('textbox', { name: '메시지', exact: true }).fill('AI 없이 저장');
  await page
    .getByRole('button', { name: '메시지 보내기' })
    .evaluate((button: HTMLButtonElement) => {
      button.click();
      button.click();
    });
  await expect(page.locator('.message.user')).toHaveCount(1);
  await expect(page.locator('.message.user')).toContainText('AI 없이 저장');
  await expect(page.locator('.message.assistant')).toHaveCount(0);
  await page.reload();
  await page.getByRole('button', { name: '새 대화', exact: true }).click();
  await expect(page.locator('.message.user')).toContainText('AI 없이 저장');
});

test('목록 페이지 경계의 대화를 삭제해도 다음 페이지 커서를 복구함', async ({ page }) => {
  await login(page, true);
  await expect(page.locator('.conversation')).toHaveCount(20);
  // Act on the boundary row without scrolling the sentinel into view first.
  await page
    .getByRole('button', { name: '페이지 대화 6 메뉴', exact: true })
    .evaluate((button: HTMLButtonElement) => button.click());
  await page
    .getByRole('button', { name: '삭제', exact: true })
    .evaluate((button: HTMLButtonElement) => button.click());
  await expect(page.getByRole('button', { name: '페이지 대화 6 메뉴', exact: true })).toHaveCount(
    0,
  );
  await expect(page.locator('.conversation')).toHaveCount(20);
  await page.locator('.conversation-list').evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(page.locator('.conversation')).toHaveCount(24);
  await expect(page.locator('.sidebar-state[role="alert"]')).toHaveCount(0);
});

test('계정 설정과 권한 저장, 재조회, 로그아웃', async ({ page }) => {
  await login(page);
  await page.getByRole('button', { name: '설정', exact: true }).click();
  await page.getByRole('combobox', { name: '응답 언어' }).selectOption('en');
  await expect(page.getByRole('combobox', { name: '응답 언어' })).toBeEnabled();
  await page.getByRole('combobox', { name: 'weather.current 권한' }).selectOption('ASK');
  await expect(page.getByRole('combobox', { name: 'weather.current 권한' })).toBeEnabled();
  await page.getByRole('button', { name: '닫기', exact: true }).click();
  await page.reload();
  await page.getByRole('button', { name: '설정', exact: true }).click();
  await expect(page.getByRole('combobox', { name: '응답 언어' })).toHaveValue('en');
  await expect(page.getByRole('combobox', { name: 'weather.current 권한' })).toHaveValue('ASK');
  await page.getByRole('button', { name: '로그아웃', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'ACE에 로그인' })).toBeVisible();
  expect(await page.evaluate(() => sessionStorage.getItem('ace-auth-session'))).toBeNull();
});

test('음성 원문은 대화에 저장하지 않고 최소 로그만 저장, 승인 취소와 차단', async ({ page }) => {
  await login(page);
  const voice = async (text: string) => {
    await page.getByRole('button', { name: '음성 입력', exact: true }).click();
    await page.getByRole('textbox', { name: '음성 명령 문장' }).fill(text);
    await page.getByRole('button', { name: '명령 실행', exact: true }).click();
  };
  await voice('메모장 실행');
  await expect(page.getByRole('dialog')).toContainText('시스템 명령 실행 확인');
  await page.getByRole('button', { name: '취소', exact: true }).click();
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await voice('레지스트리 삭제');
  await expect(page.locator('.system-action.error')).toContainText('BLOCKED');
  await voice('시스템 상태 조회');
  await expect(page.locator('.system-action.error')).toContainText('Tauri');
  const token = await page.evaluate(
    () => JSON.parse(sessionStorage.getItem('ace-auth-session')!).accessToken as string,
  );
  const logs = await page.request.get('http://127.0.0.1:3002/logs/voice', {
    headers: { Authorization: `Bearer ${token}` },
  });
  const data = await logs.json();
  expect(data.items).toHaveLength(3);
  expect(data.items.some((item: { status: string }) => item.status === 'CANCELLED')).toBe(true);
  expect(JSON.stringify(data)).not.toContain('레지스트리 삭제');
  const conversations = await page.request.get('http://127.0.0.1:3002/conversations', {
    headers: { Authorization: `Bearer ${token}` },
  });
  expect((await conversations.json()).items).toEqual([]);
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
});

test('병렬 401은 refresh 요청 하나를 공유하고 정상 요청으로 복구', async ({ page }) => {
  await login(page);
  let refreshes = 0;
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/auth/refresh') refreshes++;
  });
  await page.evaluate(async () => {
    const session = await import('/src/api/session.ts');
    const { conversationApi } = await import('/src/api/conversations.api.ts');
    const { settingsApi } = await import('/src/api/settings.api.ts');
    session.setSession({ ...session.getSession(), accessToken: 'invalid-expired-token' });
    await Promise.all([conversationApi.list(), settingsApi.get(), settingsApi.permissions()]);
  });
  expect(refreshes).toBe(1);
  await page.reload();
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
});

test('조회 실패는 오류와 재시도를 표시하고 실패 데이터를 Mock으로 대체하지 않음', async ({
  page,
}) => {
  let fail = true;
  await page.route('**/conversations?*', async (route) => {
    if (fail)
      await route.fulfill({
        status: 503,
        json: { statusCode: 503, code: 'DATABASE_UNAVAILABLE', message: 'Unavailable' },
      });
    else await route.continue();
  });
  await login(page);
  await expect(page.locator('.sidebar-state[role="alert"]')).toContainText('데이터베이스');
  await expect(page.locator('.conversation')).toHaveCount(0);
  fail = false;
  await page.getByRole('button', { name: '다시 시도', exact: true }).click();
  await expect(page.locator('.sidebar-state[role="alert"]')).not.toBeVisible();
  await expect(page.locator('.sidebar-state')).toContainText('아직 대화가 없습니다.');
});

test('대화 전환 시 이전 GET을 취소하고 늦은 메시지 응답을 새 대화에 표시하지 않음', async ({
  page,
}) => {
  const fixture = await login(page, true);
  let release!: () => void;
  let began!: () => void;
  const started = new Promise<void>((resolve) => {
    began = resolve;
  });
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(`**/conversations/${fixture.conversationId}/messages?*`, async (route) => {
    const response = await route.fetch();
    began();
    await held;
    try {
      await route.fulfill({ response });
    } catch {
      /* The request is expected to be aborted. */
    }
  });
  await page.getByRole('button', { name: '페이지 메시지', exact: true }).click();
  await started;
  await page.getByRole('button', { name: '페이지 대화 24', exact: true }).click();
  await expect(page.locator('.message-content')).toContainText('새로운 대화를 시작해 보세요.');
  release();
  await expect(page.locator('.message')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '페이지 대화 24', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
});
