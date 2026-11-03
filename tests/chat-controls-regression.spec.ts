import { test, expect } from '@playwright/test';

test('빈 입력의 이전 명령 탐색, 편집 보존 및 전송 후 하단 이동', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(async () => {
    // @ts-expect-error Vite test-only module.
    const { mount } = await import('/tests/chat-controls-harness.tsx');
    mount();
  });
  const input = page.getByRole('textbox', { name: '메시지' });
  await input.press('ArrowUp');
  await expect(input).toHaveValue('세 번째 명령');
  await input.press('ArrowUp');
  await expect(input).toHaveValue('두 번째 명령');
  await input.press('ArrowDown');
  await expect(input).toHaveValue('세 번째 명령');
  await input.press('ArrowDown');
  await expect(input).toHaveValue('');
  await input.press('ArrowUp');
  await input.press('ArrowUp');
  await input.press('ArrowUp');
  await expect(input).toHaveValue('첫 번째 명령');
  await input.press('ArrowUp');
  await expect(input).toHaveValue('이전 페이지 명령');
  await input.fill('작성 중');
  await input.press('ArrowUp');
  await expect(input).toHaveValue('작성 중');
  const scroll = page.locator('.message-area');
  await scroll.evaluate((el) => (el.scrollTop = 50));
  await expect
    .poll(() => scroll.evaluate((el) => el.scrollHeight - el.scrollTop - el.clientHeight))
    .toBeGreaterThan(100);
  await input.press('Enter');
  await expect(input).toHaveValue('');
  await expect
    .poll(() => scroll.evaluate((el) => el.scrollHeight - el.scrollTop - el.clientHeight))
    .toBeLessThan(5);
});
