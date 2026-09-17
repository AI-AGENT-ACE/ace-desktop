import { apiClient } from './client';
import type { VoiceLogInput } from '../types';
export async function recordVoiceLog(input: VoiceLogInput) {
  await apiClient.post('/logs/voice', input);
}
