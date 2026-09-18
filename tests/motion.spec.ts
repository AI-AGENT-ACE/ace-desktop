import { test, expect } from '@playwright/test';
import { login, openProfileItem } from './helpers';

test('테마는 300ms 동안 중간 색상을 거쳐 변경되고 설정에서도 적용', async ({ page }) => {
  await login(page);
  await expect(page.locator('html')).toHaveAttribute('data-motion-ready', 'true');
  await expect(page.locator('body')).toHaveCSS('transition-duration', '0.3s');
  await expect(page.locator('.sidebar')).toHaveCSS('background-color', 'rgb(242, 243, 239)');
  await openProfileItem(page, '설정');
  const middle = await page.evaluate(async () => {
    const select = document.querySelector('[aria-label="화면 테마"]') as HTMLSelectElement;
    select.value = 'dark';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 100));
    return getComputedStyle(document.querySelector('.sidebar')!).backgroundColor;
  });
  expect(middle).not.toBe('rgb(242, 243, 239)');
  expect(middle).not.toBe('rgb(25, 28, 24)');
  await expect(page.locator('.sidebar')).toHaveCSS('background-color', 'rgb(25, 28, 24)');
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
  await openProfileItem(page, '설정');
  await page.getByRole('combobox', { name: '화면 테마' }).selectOption('light');
  await expect(page.locator('.sidebar')).toHaveCSS('background-color', 'rgb(242, 243, 239)');
  await page.getByRole('button', { name: '닫기', exact: true }).click();
  await page.getByRole('button', { name: '사이드바 접기' }).click();
  await expect(page.locator('.sidebar-panel')).toHaveCSS('width', '0px');
  await expect(page.locator('.sidebar-panel')).toHaveCSS('transition-duration', '0s');
});

test('사이드바는 접기와 펼치기 모두 중간 너비를 거쳐 부드럽게 전환한다', async ({ page }) => {
  await login(page);
  const panel = page.locator('.sidebar-panel');
  await expect(panel).toHaveCSS('width', '260px');
  const closing = await page.evaluate(async () => {
    (document.querySelector('[aria-label="사이드바 접기"]') as HTMLButtonElement).click();
    await new Promise((resolve) => setTimeout(resolve, 80));
    return document.querySelector('.sidebar-panel')!.getBoundingClientRect().width;
  });
  expect(closing).toBeGreaterThan(0);
  expect(closing).toBeLessThan(260);
  await expect(panel).toHaveCSS('width', '0px');
  await expect(panel).toHaveAttribute('inert', '');
  await expect(page.getByRole('button', { name: '사이드바 펼치기' })).toBeFocused();
  const opening = await page.evaluate(async () => {
    (document.querySelector('[aria-label="사이드바 펼치기"]') as HTMLButtonElement).click();
    await new Promise((resolve) => setTimeout(resolve, 80));
    return document.querySelector('.sidebar-panel')!.getBoundingClientRect().width;
  });
  expect(opening).toBeGreaterThan(0);
  expect(opening).toBeLessThan(260);
  await expect(panel).toHaveCSS('width', '260px');
  await expect(page.getByRole('button', { name: '사이드바 접기' })).toBeFocused();
});
