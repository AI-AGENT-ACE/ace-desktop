import { expect, type Page } from '@playwright/test';
export async function login(page: Page, seed = false) {
  const response = await page.request.get(`http://127.0.0.1:3002/__test/fixture?seed=${seed}`);
  expect(response.ok()).toBe(true);
  const fixture = (await response.json()) as {
    email: string;
    password: string;
    conversationId: string | null;
  };
  await page.goto('/');
  await page.getByRole('textbox', { name: '이메일', exact: true }).fill(fixture.email);
  await page.getByLabel('비밀번호', { exact: true }).fill(fixture.password);
  await page.getByRole('button', { name: '로그인', exact: true }).click();
  await expect(page.getByRole('heading', { name: '무엇을 도와드릴까요?' })).toBeVisible();
  return fixture;
}
