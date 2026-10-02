import { authApi } from './auth.api';

export async function runConcurrentAuthRequestCapture() {
  if (!import.meta.env.DEV) throw new Error('개발 환경에서만 사용할 수 있습니다.');

  return Promise.all([authApi.me(), authApi.me(), authApi.me()]);
}
