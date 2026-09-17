import { useEffect, useRef, useState, type ReactNode } from 'react';
import { authApi } from '../../api/auth.api';
import { apiErrorMessage, isCancelled } from '../../api/client';
import { getSession, setSession } from '../../api/session';
import type { User } from '../../types';
import { CustomTitleBar } from '../../layouts/CustomTitleBar';

export function AuthGate({
  children,
}: {
  children: (user: User, logout: () => Promise<void>) => ReactNode;
}) {
  const [user, setUser] = useState<User | null>(null);
  const [checking, setChecking] = useState(!!getSession());
  const [error, setError] = useState('');
  const [register, setRegister] = useState(false);
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [busy, setBusy] = useState(false);
  const lock = useRef(false);
  useEffect(() => {
    const controller = new AbortController();
    if (getSession())
      authApi
        .me(controller.signal)
        .then(setUser)
        .catch((cause) => {
          if (!isCancelled(cause)) setError(apiErrorMessage(cause));
        })
        .finally(() => {
          if (!controller.signal.aborted) setChecking(false);
        });
    const changed = () => {
      if (!getSession()) {
        setUser(null);
        setChecking(false);
      }
    };
    window.addEventListener('ace-session-change', changed);
    return () => {
      controller.abort();
      window.removeEventListener('ace-session-change', changed);
    };
  }, []);
  const submit = async () => {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError('');
    try {
      const response = register
        ? await authApi.register(email.trim(), password, displayName)
        : await authApi.login(email.trim(), password);
      setSession(response);
      setPassword('');
      setUser(response.user);
    } catch (cause) {
      setError(apiErrorMessage(cause));
    } finally {
      lock.current = false;
      setBusy(false);
    }
  };
  const logout = async () => {
    try {
      await authApi.logout();
    } catch {
      /* Local credentials must also be discarded when offline. */
    } finally {
      setSession(null);
      setUser(null);
    }
  };
  if (user) return children(user, logout);
  return (
    <div className="app">
      <CustomTitleBar onNotice={setError} />
      <main className="auth-screen">
        <div className="auth-card">
          <img src="/ace-logo.png" alt="ACE" />
          <h1>{checking ? '로그인 확인 중…' : register ? 'ACE 시작하기' : 'ACE에 로그인'}</h1>
          {!checking && (
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void submit();
              }}
            >
              {register && (
                <>
                  <label className="field-label" htmlFor="display-name">
                    표시 이름
                  </label>
                  <input
                    id="display-name"
                    className="text-field"
                    maxLength={80}
                    value={displayName}
                    onChange={(event) => setDisplayName(event.target.value)}
                  />
                </>
              )}
              <label className="field-label" htmlFor="email">
                이메일
              </label>
              <input
                id="email"
                className="text-field"
                type="email"
                autoComplete="username"
                required
                maxLength={254}
                value={email}
                onChange={(event) => setEmail(event.target.value)}
              />
              <label className="field-label" htmlFor="password">
                비밀번호
              </label>
              <input
                id="password"
                className="text-field"
                type="password"
                autoComplete={register ? 'new-password' : 'current-password'}
                required
                minLength={12}
                maxLength={128}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
              {register && <p className="modal-description">비밀번호는 12~128자로 입력하세요.</p>}
              <button className="primary auth-submit" disabled={busy}>
                {busy ? '처리 중…' : register ? '회원가입' : '로그인'}
              </button>
              <button
                type="button"
                className="auth-submit"
                disabled={busy}
                onClick={() => {
                  setRegister(!register);
                  setError('');
                }}
              >
                {register ? '로그인으로 돌아가기' : '계정 만들기'}
              </button>
            </form>
          )}
          {error && (
            <p className="danger-text" role="alert">
              {error}
            </p>
          )}
        </div>
      </main>
    </div>
  );
}
