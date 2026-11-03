import { test, expect } from '@playwright/test';

test('거절·번호 변형은 거절로 전달하고 불확실한 말은 승인하지 않음', async ({ page }) => {
  await page.goto('/');
  const result = await page.evaluate(async () => {
    // @ts-expect-error Vite source module.
    const { resolveVoiceChoice } = await import('/src/features/voice/voice-session-policy.ts');
    const choices = [
      { id: 'yes', label: '닫기' },
      { id: 'no', label: '취소' },
    ];
    return {
      negatives: [
        '취소',
        '“취소”',
        '취소해 주세요.',
        '아뇨',
        '아니요 아니요',
        '2번이요',
        '이번요',
      ].map((t) => resolveVoiceChoice(t, choices)),
      uncertain: ['', '네 아니요', '취소하지 마', '최소', '잘 모르겠어', '4번'].map((t) =>
        resolveVoiceChoice(t, choices),
      ),
      positive: resolveVoiceChoice('네', choices),
      selection: resolveVoiceChoice('2번', [
        { id: 'window-a', label: '창 A' },
        { id: 'window-b', label: '창 B' },
      ]),
    };
  });
  expect(result.negatives).toEqual(Array(7).fill('no'));
  expect(result.uncertain).toEqual(Array(6).fill(null));
  expect(result.positive).toBe('yes');
  expect(result.selection).toBe('window-b');
});
