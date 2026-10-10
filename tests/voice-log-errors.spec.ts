import { test, expect } from '@playwright/test';
import { normalizeVoiceLogErrorCode } from '../src/api/voice-log-errors';

test('음성 로그 오류 코드를 백엔드 허용 값으로 정규화한다', () => {
  expect(normalizeVoiceLogErrorCode('AMBIGUOUS_APP_MATCH')).toBe('EXECUTION_FAILED');
  expect(normalizeVoiceLogErrorCode('INVALID_ARGUMENT')).toBe('INVALID_ARGUMENTS');
  expect(normalizeVoiceLogErrorCode('BLOCKED_BY_POLICY')).toBe('BLOCKED');
  expect(normalizeVoiceLogErrorCode('CANCELLED')).toBe('CANCELLED');
  expect(normalizeVoiceLogErrorCode(undefined)).toBeUndefined();
});
