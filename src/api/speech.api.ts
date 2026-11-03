import { apiClient } from './client';
import { settingsApi } from './settings.api';

let generation = 0;
let active: HTMLAudioElement | null = null;
let objectUrl: string | null = null;
let request: AbortController | null = null;

export function stopSpeech() {
  generation += 1;
  request?.abort();
  request = null;
  active?.pause();
  active = null;
  if (objectUrl) URL.revokeObjectURL(objectUrl);
  objectUrl = null;
}

export async function speakAssistant(text: string) {
  stopSpeech();
  const current = generation;
  if (!text || !(await settingsApi.get()).ttsEnabled || current !== generation) return;
  // Read prose, not Markdown/HTML formatting markers.
  text = text
    .replace(/<br\s*\/?\s*>/gi, '\n')
    .replace(/<[^>]+>/g, '')
    .replace(/```[\s\S]*?```/g, ' 코드 예시는 화면에서 확인해 주세요. ')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/[*_`#]/g, '')
    .trim();
  if (!text) return;
  request = new AbortController();
  const signal = request.signal;
  try {
    const response = await apiClient.post<Blob>(
      '/voice/tts',
      { text: text.slice(0, 3000) },
      { responseType: 'blob', timeout: 150000, signal },
    );
    if (current !== generation) return;
    objectUrl = URL.createObjectURL(response.data);
    active = new Audio(objectUrl);
    active.volume = 1;
    active.onended = () => {
      if (current === generation) stopSpeech();
    };
    active.onerror = () => {
      if (current === generation) stopSpeech();
    };
    await active.play();
  } catch (error) {
    if (signal.aborted || current !== generation) return;
    stopSpeech();
    throw error;
  }
}
