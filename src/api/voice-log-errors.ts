import type { VoiceLogErrorCode } from '../types';

const allowedCodes = new Set<VoiceLogErrorCode>([
  'BLOCKED',
  'EXECUTION_FAILED',
  'DESKTOP_REQUIRED',
  'CANCELLED',
  'CONFIRMATION_REQUIRED',
  'INVALID_ARGUMENTS',
]);

export function normalizeVoiceLogErrorCode(code?: string): VoiceLogErrorCode | undefined {
  if (!code) return undefined;
  if (code === 'INVALID_ARGUMENT') return 'INVALID_ARGUMENTS';
  if (code === 'BLOCKED_BY_POLICY') return 'BLOCKED';
  return allowedCodes.has(code as VoiceLogErrorCode)
    ? (code as VoiceLogErrorCode)
    : 'EXECUTION_FAILED';
}
