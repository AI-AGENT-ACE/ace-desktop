import { useEffect, useRef, useState } from 'react';
import axios from 'axios';
import { authApi } from '../../../api/auth.api';
import { isCancelled } from '../../../api/client';
import { getSession, setSession } from '../../../api/session';
import type { User } from '../../../types';

type AuthState =
  | { status: 'checking' | 'unauthenticated'; user: null }
  | { status: 'authenticated'; user: User };
export interface AuthInput {
  email: string;
  password: string;
  displayName: string;
  rememberMe: boolean;
}
export type AuthResult = 'registered' | 'authenticated' | 'failed';
function authErrorMessage(cause: unknown) {
  if (!axios.isAxiosError(cause)) return '인증을 완료하지 못했습니다. 다시 시도해 주세요.';
  if (!cause.response) return '서버에 연결할 수 없습니다. 잠시 후 다시 시도해 주세요.';
  if (cause.response.status === 401) return '이메일 또는 비밀번호를 확인해 주세요.';
  if (cause.response.status === 409) return '이미 가입된 이메일입니다. 로그인해 주세요.';
  if (cause.response.status === 400) return '입력한 정보의 형식과 길이를 확인해 주세요.';
  if (cause.response.status === 429) return '요청이 많습니다. 잠시 후 다시 시도해 주세요.';
  return '서버에서 인증을 처리하지 못했습니다. 잠시 후 다시 시도해 주세요.';
}
export function useAuth() {
  const [state, setState] = useState<AuthState>({ status: 'checking', user: null });
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const lock = useRef(false);
  useEffect(() => {
    const controller = new AbortController();
    const changed = () => {
      if (!getSession()) {
        controller.abort();
        setState({ status: 'unauthenticated', user: null });
      }
    };
    window.addEventListener('ace-session-change', changed);
    if (!getSession()) setState({ status: 'unauthenticated', user: null });
    else
      void authApi
        .me(controller.signal)
        .then((user) => {
          if (!controller.signal.aborted && getSession())
            setState({ status: 'authenticated', user });
        })
        .catch((cause) => {
          if (!isCancelled(cause) && !controller.signal.aborted) {
            setError(authErrorMessage(cause));
            setState({ status: 'unauthenticated', user: null });
          }
        });
    return () => {
      controller.abort();
      window.removeEventListener('ace-session-change', changed);
    };
  }, []);
  const authenticate = async (mode: 'login' | 'signup', input: AuthInput) => {
    if (lock.current) return 'failed' as const;
    lock.current = true;
    setBusy(true);
    setError('');
    try {
      if (mode === 'signup') {
        const response = await authApi.register(
          input.email.trim(),
          input.password,
          input.displayName,
        );
        const { user: _user, ...tokens } = response;
        setSession(tokens, false);
        try {
          await authApi.logout();
        } catch {
          // 가입 성공 자체는 유지하고, 발급받은 임시 토큰은 로컬에서 폐기한다.
        } finally {
          setSession(null);
        }
        setState({ status: 'unauthenticated', user: null });
        return 'registered' as const;
      }
      const response = await authApi.login(input.email.trim(), input.password);
      const { user, ...tokens } = response;
      setSession(tokens, input.rememberMe);
      setState({ status: 'authenticated', user });
      return 'authenticated' as const;
    } catch (cause) {
      setError(authErrorMessage(cause));
      return 'failed' as const;
    } finally {
      lock.current = false;
      setBusy(false);
    }
  };
  const logout = async () => {
    try {
      await authApi.logout();
    } catch {
      /* Offline logout still discards local credentials. */
    } finally {
      setSession(null);
      setState({ status: 'unauthenticated', user: null });
      setError('');
    }
  };
  return { state, error, busy, authenticate, logout, clearError: () => setError('') };
}
