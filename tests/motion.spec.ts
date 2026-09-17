import { test, expect } from '@playwright/test';
import { login } from './helpers';

test('테마는 300ms 동안 중간 색상을 거쳐 변경되고 설정에서도 적용', async ({ page }) => {
  await login(page);
  await expect(page.locator('html')).toHaveAttribute('data-motion-ready', 'true');
  await expect(page.locator('body')).toHaveCSS('transition-duration', '0.3s');
  await expect(page.locator('.sidebar')).toHaveCSS('background-color', 'rgb(242, 243, 239)');
  const middle = await page.evaluate(async () => {
    (document.querySelector('[aria-label="다크 테마 적용"]') as HTMLButtonElement).click();
    await new Promise((resolve) => setTimeout(resolve, 100));
    return getComputedStyle(document.querySelector('.sidebar')!).backgroundColor;
  });
  expect(middle).not.toBe('rgb(242, 243, 239)');
  expect(middle).not.toBe('rgb(25, 28, 24)');
  await expect(page.locator('.sidebar')).toHaveCSS('background-color', 'rgb(25, 28, 24)');
  await page.getByRole('button', { name: '설정', exact: true }).click();
  await page.getByRole('combobox', { name: '화면 테마' }).selectOption('light');
  await expect(page.locator('.sidebar')).toHaveCSS('background-color', 'rgb(242, 243, 239)');
});

test('저장된 테마로 시작하고 reduced motion은 전환을 제거', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.addInitScript(() => localStorage.setItem('ace-theme', 'dark'));
  await login(page);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(page.locator('html')).toHaveAttribute('data-motion-ready', 'true');
  await expect(page.locator('body')).toHaveCSS('transition-duration', '0s');
  await page.getByRole('button', { name: '라이트 테마 적용' }).click();
  await expect(page.locator('.sidebar')).toHaveCSS('background-color', 'rgb(242, 243, 239)');
});
