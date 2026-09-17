import axios, { AxiosError, CanceledError, type InternalAxiosRequestConfig } from 'axios';
import { getSession, setSession } from './session';
import { recordRequestMetric } from './metrics';
import type { ApiErrorResponse, TokenPair } from '../types';
declare module 'axios' {
  interface AxiosRequestConfig {
    skipAuth?: boolean;
    retried?: boolean;
    baselineStartedAt?: number;
  }
}
const baseURL = import.meta.env.VITE_API_BASE_URL?.replace(/\/$/, '');
if (!baseURL || !/^https?:\/\//.test(baseURL))
  throw new Error('VITE_API_BASE_URL에 HTTP(S) 백엔드 주소를 설정하세요.');
export const apiClient = axios.create({
  baseURL,
  timeout: 10000,
  headers: { 'Content-Type': 'application/json' },
});
let refreshFlight: Promise<TokenPair> | null = null;
apiClient.interceptors.request.use((config) => {
  if (import.meta.env.DEV) config.baselineStartedAt = performance.now();
  if (!config.skipAuth) {
    const session = getSession();
    if (session) config.headers.Authorization = `Bearer ${session.accessToken}`;
  }
  return config;
});
async function refreshSession() {
  if (refreshFlight) return refreshFlight;
  const previous = getSession();
  if (!previous) throw new Error('로그인이 필요합니다.');
  refreshFlight = apiClient
    .post<TokenPair>('/auth/refresh', { refreshToken: previous.refreshToken }, { skipAuth: true })
    .then(({ data }) => {
      if (getSession()?.refreshToken !== previous.refreshToken)
        throw new CanceledError('Session changed');
      setSession(data);
      return data;
    })
    .catch((error: unknown) => {
      if (getSession()?.refreshToken === previous.refreshToken) setSession(null);
      throw error;
    })
    .finally(() => {
      refreshFlight = null;
    });
  return refreshFlight;
}
apiClient.interceptors.response.use(
  (response) => {
    recordRequestMetric(response.config, response.status);
    return response;
  },
  async (error: AxiosError) => {
    const config: InternalAxiosRequestConfig | undefined = error.config;
    recordRequestMetric(config, error.response?.status ?? null, axios.isCancel(error));
    if (config?.signal?.aborted) throw new CanceledError();
    if (
      error.response?.status === 401 &&
      config?.retried &&
      !config.skipAuth &&
      config.headers.Authorization === `Bearer ${getSession()?.accessToken}`
    )
      setSession(null);
    if (
      error.response?.status !== 401 ||
      !config ||
      config.skipAuth ||
      config.retried ||
      !getSession()
    )
      throw error;
    config.retried = true;
    const current = getSession()!;
    if (config.headers.Authorization === `Bearer ${current.accessToken}`) await refreshSession();
    if (config.signal?.aborted) throw new CanceledError();
    return apiClient.request(config);
  },
);
export const isCancelled = (error: unknown) => axios.isCancel(error);
const messages: Record<string, string> = {
  INVALID_CREDENTIALS: '이메일 또는 비밀번호가 올바르지 않습니다.',
  EMAIL_ALREADY_REGISTERED: '이미 가입된 이메일입니다.',
  AI_NOT_CONFIGURED: 'AI 서버가 아직 연결되지 않았습니다. 입력한 메시지는 서버에 저장됩니다.',
  AI_UNAVAILABLE: 'AI 서버에 연결하지 못했습니다. 저장된 메시지를 확인해 주세요.',
  WEATHER_NOT_CONFIGURED: '날씨 API Key가 설정되지 않았습니다.',
  RATE_LIMITED: '요청이 많습니다. 잠시 후 다시 시도해 주세요.',
  UNAUTHENTICATED: '로그인이 만료되었습니다. 다시 로그인해 주세요.',
  DATABASE_UNAVAILABLE: '데이터베이스에 연결하지 못했습니다.',
};
export function apiErrorMessage(error: unknown): string {
  if (axios.isAxiosError<ApiErrorResponse>(error)) {
    const data = error.response?.data;
    if (data?.code && messages[data.code]) return messages[data.code];
    if (data?.message) return Array.isArray(data.message) ? data.message.join(' · ') : data.message;
    if (!error.response)
      return '백엔드에 연결하지 못했습니다. 서버 실행 상태와 API 주소를 확인해 주세요.';
  }
  return error instanceof Error ? error.message : '작업을 완료하지 못했습니다.';
}
