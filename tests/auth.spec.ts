import { test, expect, type Page } from '@playwright/test';

const user = {
  id: 'auth-ui-user',
  email: 'auth@example.com',
  displayName: '인증 테스트',
  createdAt: '',
  updatedAt: '',
};
const tokens = {
  accessToken: 'test-access',
  refreshToken: 'test-refresh',
  tokenType: 'Bearer',
  expiresIn: 900,
  refreshExpiresAt: '',
};
async function mockMain(page: Page) {
  await page.route('http://127.0.0.1:3002/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    const data =
      path === '/health'
        ? { ai: 'not-configured' }
        : path === '/users/me'
          ? user
          : path === '/settings'
            ? { responseLanguage: 'ko', ttsEnabled: false }
            : { items: [], hasMore: false, nextCursor: null };
    await route.fulfill({ json: data });
  });
}
async function fillLogin(page: Page) {
  await page.getByLabel('이메일', { exact: true }).fill(user.email);
  await page.getByLabel('비밀번호', { exact: true }).fill('TestPassword123!');
}

test('미인증에서는 메인 API를 호출하지 않고 작은 창과 다크 테마를 지원한다', async ({ page }) => {
  const requests: string[] = [];
  page.on('request', (request) => {
    if (request.url().includes(':3002/')) requests.push(request.url());
  });
  await page.addInitScript(() => localStorage.setItem('ace-theme', 'dark'));
  await page.setViewportSize({ width: 420, height: 600 });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'ACE에 로그인' })).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('button', { name: '계정 만들기' }).click();
  await expect(page.getByLabel('비밀번호 확인')).toBeVisible();
  await page.setViewportSize({ width: 1200, height: 800 });
  await page.locator('.auth-screen').evaluate((element) => {
    element.scrollTop = 0;
  });
  await expect(page.getByRole('button', { name: '회원가입', exact: true })).toBeInViewport();
  await expect(page.getByRole('button', { name: '로그인으로 돌아가기' })).toBeInViewport();
  await page.screenshot({ path: 'artifacts/auth-signup-dark.png' });
  await page.getByRole('button', { name: '로그인으로 돌아가기' }).click();
  expect(requests).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('비밀번호 확인 불일치는 API 호출을 막고 전환하면 비밀번호를 지운다', async ({ page }) => {
  let calls = 0;
  await page.route('**/auth/register', (route) => {
    calls++;
    return route.fulfill({ json: { ...tokens, user } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '계정 만들기' }).click();
  await fillLogin(page);
  await page.getByLabel('비밀번호 확인').fill('AnotherPassword123!');
  await page.getByRole('button', { name: '회원가입', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('비밀번호가 일치하지 않습니다.');
  await expect(page.getByLabel('비밀번호 확인')).toBeFocused();
  expect(calls).toBe(0);
  await page.getByRole('button', { name: '로그인으로 돌아가기' }).click();
  await expect(page.getByLabel('비밀번호', { exact: true })).toHaveValue('');
});

test('가입은 실제 DTO 필드만 보내고 성공하면 메인으로 진입한다', async ({ page }) => {
  await mockMain(page);
  let body: Record<string, unknown> = {};
  await page.route('**/auth/register', async (route) => {
    body = route.request().postDataJSON();
    await route.fulfill({ json: { ...tokens, user } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '계정 만들기' }).click();
  await fillLogin(page);
  await page.getByLabel('비밀번호 확인').fill('TestPassword123!');
  await page.getByRole('button', { name: '회원가입', exact: true }).click();
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
  expect(Object.keys(body).sort()).toEqual(['email', 'password']);
  const saved = await page.evaluate(() => JSON.parse(sessionStorage.getItem('ace-auth-session')!));
  expect(saved.user).toBeUndefined();
  expect(saved.password).toBeUndefined();
});

test('Enter 제출과 중복 제출 방지 및 비밀번호 표시를 지원한다', async ({ page }) => {
  await mockMain(page);
  let calls = 0;
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/auth/login', async (route) => {
    calls++;
    await held;
    await route.fulfill({ json: { ...tokens, user } });
  });
  await page.goto('/');
  await fillLogin(page);
  await page.getByRole('button', { name: '비밀번호 표시', exact: true }).click();
  await expect(page.getByLabel('비밀번호', { exact: true })).toHaveAttribute('type', 'text');
  await page.getByLabel('비밀번호', { exact: true }).press('Enter');
  await expect(page.getByRole('button', { name: '로그인 중…' })).toBeDisabled();
  await page.locator('form').evaluate((form) => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  });
  expect(calls).toBe(1);
  release();
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
});

test('자격 증명 오류와 네트워크 오류를 구분하고 raw 오류는 숨긴다', async ({ page }) => {
  await page.route('**/auth/login', (route) =>
    route.fulfill({ status: 401, json: { message: 'RAW_SERVER_SECRET', stack: 'INTERNAL_TRACE' } }),
  );
  await page.goto('/');
  await fillLogin(page);
  await page.getByRole('button', { name: '로그인', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('이메일 또는 비밀번호를 확인해 주세요.');
  await page.route('**/auth/login', (route) => route.abort('connectionrefused'));
  await page.getByRole('button', { name: '로그인', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('서버에 연결할 수 없습니다.');
  await expect(page.locator('body')).not.toContainText('RAW_SERVER_SECRET');
});

test('복원 확인 중에는 메인 요청과 화면을 노출하지 않는다', async ({ page }) => {
  await mockMain(page);
  await page.addInitScript(
    (pair) => sessionStorage.setItem('ace-auth-session', JSON.stringify(pair)),
    tokens,
  );
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const mainRequests: string[] = [];
  page.on('request', (request) => {
    if (
      request.url().includes(':3002/') &&
      /\/conversations|\/settings|\/permissions/.test(request.url())
    )
      mainRequests.push(request.url());
  });
  await page.route('**/users/me', async (route) => {
    await held;
    await route.fulfill({ json: user });
  });
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('로그인 확인 중…');
  await expect(page.locator('.sidebar')).toHaveCount(0);
  expect(mainRequests).toEqual([]);
  release();
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
});

test('갱신 실패는 저장된 세션과 메인 화면을 제거한다', async ({ page }) => {
  await mockMain(page);
  await page.addInitScript(
    (pair) => sessionStorage.setItem('ace-auth-session', JSON.stringify(pair)),
    tokens,
  );
  await page.route('**/users/me', (route) => route.fulfill({ status: 401, json: {} }));
  await page.route('**/auth/refresh', (route) => route.fulfill({ status: 401, json: {} }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'ACE에 로그인' })).toBeVisible();
  expect(await page.evaluate(() => sessionStorage.getItem('ace-auth-session'))).toBeNull();
  await expect(page.locator('.sidebar')).toHaveCount(0);
});

test('가입 비밀번호는 최소 8자와 영문·숫자·기호를 요구하고 기본 표시 버튼은 숨긴다', async ({
  page,
}) => {
  await mockMain(page);
  let calls = 0;
  await page.route('**/auth/register', (route) => {
    calls++;
    return route.fulfill({ json: { ...tokens, user } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '계정 만들기' }).click();
  await page.getByLabel('이메일', { exact: true }).fill(user.email);
  const password = page.getByLabel('비밀번호', { exact: true });
  for (const candidate of [
    'Abcde1!',
    'abcdefgh',
    'abcdefg1',
    '1234567!',
    'abcdefg!',
    'Abcdef1 ',
    'Abcdef1한',
  ]) {
    await password.fill(candidate);
    await page.getByLabel('비밀번호 확인').fill(candidate);
    await page.getByRole('button', { name: '회원가입', exact: true }).click();
    expect(calls).toBe(0);
  }
  await password.fill('');
  await password.pressSequentially('Abcdef1!');
  await page.screenshot({ path: 'artifacts/auth-password-reveal.png' });
  await expect(page.getByRole('button', { name: '비밀번호 표시', exact: true })).toBeVisible();
  await page.getByLabel('비밀번호 확인').fill('Abcdef1!');
  await page.getByRole('button', { name: '회원가입', exact: true }).click();
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
  expect(calls).toBe(1);
});
