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
  UNSUPPORTED_FILE_TYPE: '지원하지 않는 파일 형식입니다.',
  FILE_TOO_LARGE: '파일 크기 제한을 초과했습니다.',
  INVALID_FILE_SIGNATURE: '파일 내용과 확장자가 일치하지 않습니다.',
  FILE_UPLOAD_FAILED: '파일을 업로드하지 못했습니다.',
  FILE_DELETE_FAILED: '파일을 삭제하지 못했습니다.',
  ATTACHMENT_NOT_FOUND: '첨부 파일을 찾을 수 없습니다.',
  APP_NOT_FOUND: '설치된 앱 목록에서 요청한 앱을 찾을 수 없습니다.',
  AMBIGUOUS_APP: '같은 별칭을 사용하는 앱이 여러 개입니다.',
  STALE_APP_PATH: '앱 실행 경로가 변경되었습니다. ACE를 다시 시작해 주세요.',
  DUPLICATE_REQUEST: '동일한 앱 실행 요청이 이미 처리 중입니다.',
  ELEVATION_REQUIRED: '관리자 권한이 필요한 앱은 ACE에서 자동 실행할 수 없습니다.',
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
