import { test, expect } from '@playwright/test';

test('Orb renders Markdown within bounded scrolling space and restarts recording on click', async ({
  page,
}) => {
  await page.setViewportSize({ width: 520, height: 640 });
  await page.goto('/tests/fixtures/orb-overlay.html');
  await expect(page.getByRole('heading', { name: '서울 여행', exact: true })).toBeVisible();
  await expect(page.locator('.voice-orb-answer strong')).toHaveText('준비물');
  await expect(page.locator('.voice-orb-answer ul li')).toHaveCount(2);
  const box = page.locator('.voice-orb-bubble');
  const dimensions = await box.boundingBox();
  expect(dimensions!.width).toBeGreaterThan(288);
  expect(dimensions!.width).toBeLessThanOrEqual(460);
  expect(dimensions!.height).toBeLessThanOrEqual(430);
  expect(dimensions!.y).toBeGreaterThanOrEqual(0);
  expect(dimensions!.x + dimensions!.width).toBeLessThanOrEqual(520);
  const answer = page.locator('.voice-orb-answer');
  expect(await answer.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(
    true,
  );
  await answer.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(page.getByRole('heading', { name: '30. 여행 안내', exact: true })).toBeVisible();
  await page
    .getByRole('button', { name: 'ACE Orb, 클릭하여 음성 명령 시작 또는 드래그하여 이동' })
    .click();
  await expect(page.getByText('듣고 있어요. 말씀해 주세요.', { exact: true })).toBeVisible();
  await page
    .getByRole('button', { name: 'ACE Orb, 클릭하여 녹음 종료 또는 드래그하여 이동' })
    .press('Enter');
  await expect(page.getByRole('heading', { name: '서울 여행', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '음성 입력 닫기' }).click();
  await expect(page.locator('.voice-orb-overlay')).toHaveCount(0);
});

test('Orb bubble fits a smaller screen without hiding close or record controls', async ({
  page,
}) => {
  await page.setViewportSize({ width: 380, height: 480 });
  await page.goto('/tests/fixtures/orb-overlay.html');
  const box = await page.locator('.voice-orb-bubble').boundingBox();
  expect(box!.y).toBeGreaterThanOrEqual(0);
  expect(box!.height).toBeLessThanOrEqual(290);
  await expect(page.getByRole('button', { name: '음성 입력 닫기' })).toBeVisible();
  await expect(page.getByRole('button', { name: /클릭하여 음성 명령 시작/ })).toBeVisible();
});
