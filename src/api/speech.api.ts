import { apiClient } from './client';
import { settingsApi } from './settings.api';

let generation = 0;
let active: HTMLAudioElement | null = null;
let objectUrl: string | null = null;

export function stopSpeech() {
  generation += 1;
  active?.pause();
  active = null;
  if (objectUrl) URL.revokeObjectURL(objectUrl);
  objectUrl = null;
}

export async function speakAssistant(text: string) {
  stopSpeech();
  const current = generation;
  if (!text || !(await settingsApi.get()).ttsEnabled || current !== generation) return;
  const response = await apiClient.post<Blob>(
    '/voice/tts',
    { text: text.slice(0, 3000) },
    { responseType: 'blob', timeout: 150000 },
  );
  if (current !== generation) return;
  objectUrl = URL.createObjectURL(response.data);
  active = new Audio(objectUrl);
  active.onended = () => {
    if (current === generation) stopSpeech();
  };
  active.onerror = () => {
    if (current === generation) stopSpeech();
  };
  try {
    await active.play();
  } catch (error) {
    if (current === generation) stopSpeech();
    throw error;
  }
}
