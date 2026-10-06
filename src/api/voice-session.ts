import { emitTo, listen } from '@tauri-apps/api/event';
import { apiClient } from './client';
import { getSession } from './session';

// Only main owns refresh tokens and refresh rotation. Never broadcast credentials.
export function serveVoiceSession() {
  return listen<string>('ace-voice-auth-request', async ({ payload: requestId }) => {
    if (typeof requestId !== 'string' || !/^[a-f0-9-]{36}$/.test(requestId)) return;
    let accessToken: string | null = null;
    let error = '로그인이 만료되었습니다. 메인 창에서 다시 로그인해 주세요.';
    try {
      if (getSession()) {
        // The normal interceptor refreshes expired access tokens in main.
        await apiClient.get('/users/me');
        accessToken = getSession()?.accessToken ?? null;
      }
    } catch {
      if (getSession()) error = '음성 인증을 확인하지 못했습니다. 서버 연결을 확인해 주세요.';
    }
    await emitTo('voice', 'ace-voice-auth-response', { requestId, accessToken, error }).catch(
      () => undefined,
    );
  });
}

export async function requestVoiceAccessToken(): Promise<string> {
  const requestId = crypto.randomUUID();
  let unlisten: (() => void) | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let finished = false;
  try {
    return await new Promise<string>((resolve, reject) => {
      timer = setTimeout(
        () => reject(new Error('음성 인증을 확인하지 못했습니다. 서버 연결을 확인해 주세요.')),
        25000,
      );
      void listen<{ requestId: string; accessToken: string | null; error?: string }>(
        'ace-voice-auth-response',
        ({ payload }) => {
          if (payload.requestId !== requestId) return;
          if (payload.accessToken) resolve(payload.accessToken);
          else
            reject(
              new Error(
                payload.error || '로그인이 만료되었습니다. 메인 창에서 다시 로그인해 주세요.',
              ),
            );
        },
      )
        .then((stop) => {
          if (finished) {
            stop();
            return;
          }
          unlisten = stop;
          return emitTo('main', 'ace-voice-auth-request', requestId);
        })
        .catch(reject);
    });
  } finally {
    finished = true;
    clearTimeout(timer);
    unlisten?.();
  }
}
