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
  await expect(page.locator('.auth-screen')).toHaveCSS('background-color', 'rgb(0, 9, 14)');
  await expect(page.locator('.auth-card')).toHaveCSS('box-shadow', 'none');
  await expect(page.locator('.auth-brand strong')).toHaveCount(0);
  await expect(page.locator('.auth-brand img')).toHaveCSS('width', '68px');
  await expect(page.locator('.auth-card')).toHaveCSS('border-top-width', '0px');
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

test('라이트 인증 화면은 흰 배경과 검정 CTA 및 포커스를 사용한다', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('.auth-screen')).toHaveCSS('background-color', 'rgb(255, 255, 255)');
  await expect(page.getByRole('button', { name: '로그인', exact: true })).toHaveCSS(
    'background-color',
    'rgb(17, 17, 17)',
  );
  const email = page.getByLabel('이메일', { exact: true });
  await email.focus();
  await expect(email).toHaveCSS('border-color', 'rgb(17, 17, 17)');
  await page.screenshot({ path: 'artifacts/auth-login-light.png' });
  await page.getByRole('button', { name: '계정 만들기' }).click();
  await page.mouse.move(0, 0);
  await expect(page.getByRole('button', { name: '회원가입', exact: true })).toHaveCSS(
    'background-color',
    'rgb(17, 17, 17)',
  );
  await page.screenshot({ path: 'artifacts/auth-signup-light.png' });
});

test('비밀번호 확인 불일치는 API 호출을 막고 전환하면 비밀번호를 지운다', async ({ page }) => {
  let calls = 0;
  await page.route('**/auth/register', (route) => {
    calls++;
    return route.fulfill({ json: { ...tokens, user } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '계정 만들기' }).click();
  await page.getByLabel('닉네임').fill('인증 테스트');
  await fillLogin(page);
  await page.getByLabel('비밀번호 확인').fill('AnotherPassword123!');
  await page.getByRole('button', { name: '회원가입', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('비밀번호가 일치하지 않습니다.');
  await expect(page.getByLabel('비밀번호 확인')).toBeFocused();
  expect(calls).toBe(0);
  await page.getByRole('button', { name: '로그인으로 돌아가기' }).click();
  await expect(page.getByLabel('비밀번호', { exact: true })).toHaveValue('');
});

test('가입은 닉네임을 전송하고 세션을 남기지 않은 채 로그인 화면으로 돌아간다', async ({
  page,
}) => {
  await mockMain(page);
  let body: Record<string, unknown> = {};
  await page.route('**/auth/register', async (route) => {
    body = route.request().postDataJSON();
    await route.fulfill({ json: { ...tokens, user } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '계정 만들기' }).click();
  await page.getByLabel('닉네임').fill('인증 테스트');
  await fillLogin(page);
  await page.getByLabel('비밀번호 확인').fill('TestPassword123!');
  await page.getByRole('button', { name: '회원가입', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'ACE에 로그인' })).toBeVisible();
  await expect(page.getByRole('status')).toContainText('회원가입이 완료되었습니다.');
  await expect(page.getByLabel('이메일', { exact: true })).toHaveValue(user.email);
  expect(body).toEqual({
    email: user.email,
    password: 'TestPassword123!',
    displayName: '인증 테스트',
  });
  expect(await page.evaluate(() => sessionStorage.getItem('ace-auth-session'))).toBeNull();
  expect(await page.evaluate(() => localStorage.getItem('ace-auth-session'))).toBeNull();
});

test('자동 로그인은 JWT를 영구 저장하고 앱을 다시 열어도 사용자 세션을 복원한다', async ({
  page,
}) => {
  await mockMain(page);
  await page.route('**/auth/login', (route) => route.fulfill({ json: { ...tokens, user } }));
  await page.goto('/');
  await fillLogin(page);
  await page.getByLabel('자동 로그인').check();
  await page.getByRole('button', { name: '로그인', exact: true }).click();
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('ace-auth-session')!));
  expect(saved).toEqual(tokens);
  expect(await page.evaluate(() => sessionStorage.getItem('ace-auth-session'))).toBeNull();

  const rotated = {
    ...tokens,
    accessToken: 'renewed-access',
    refreshToken: 'renewed-refresh',
  };
  await page.route('**/auth/refresh', (route) => route.fulfill({ json: rotated }));
  await page.route('**/users/me', (route) => {
    const authorization = route.request().headers().authorization;
    return authorization === `Bearer ${rotated.accessToken}`
      ? route.fulfill({ json: user })
      : route.fulfill({ status: 401, json: {} });
  });
  await page.evaluate(() => {
    const current = JSON.parse(localStorage.getItem('ace-auth-session')!);
    localStorage.setItem(
      'ace-auth-session',
      JSON.stringify({ ...current, accessToken: 'expired-access' }),
    );
  });
  await page.reload();
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
  await expect(page.getByText(user.displayName, { exact: true })).toBeVisible();
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('ace-auth-session')!))).toEqual(
    rotated,
  );
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
  await expect(page.getByLabel('닉네임')).toHaveAttribute('required', '');
  await page.getByLabel('닉네임').fill('인증 테스트');
  await page.getByLabel('이메일', { exact: true }).fill(user.email);
  const password = page.getByLabel('비밀번호', { exact: true });
  await expect(page.getByRole('list', { name: '비밀번호 조건' })).toHaveCount(0);
  await password.focus();
  const rules = page.getByRole('list', { name: '비밀번호 조건' });
  await expect(rules.getByRole('listitem')).toHaveCount(4);
  await expect(rules.locator('.unmet').first()).toHaveCSS('color', 'rgb(197, 72, 72)');
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
  await password.blur();
  await expect(rules.locator('.met')).toHaveCount(4);
  await expect(rules.locator('.unmet')).toHaveCount(0);
  await expect(rules.locator('.met').first()).toHaveCSS('color', 'rgb(25, 135, 84)');
  await page.screenshot({ path: 'artifacts/auth-password-reveal.png' });
  await expect(page.getByRole('button', { name: '비밀번호 표시', exact: true })).toBeVisible();
  const confirmation = page.getByLabel('비밀번호 확인');
  await confirmation.fill('Abcdef1?');
  await expect(confirmation).toHaveCSS('border-color', 'rgb(197, 72, 72)');
  await expect(page.getByText('비밀번호가 일치하지 않습니다.')).toBeVisible();
  await confirmation.fill('Abcdef1!');
  await expect(confirmation).toHaveCSS('border-color', 'rgb(25, 135, 84)');
  await expect(page.getByText('비밀번호가 일치합니다.')).toBeVisible();
  await page.getByRole('button', { name: '회원가입', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'ACE에 로그인' })).toBeVisible();
  await expect(page.getByLabel('이메일', { exact: true })).toHaveValue(user.email);
  expect(calls).toBe(1);
});
