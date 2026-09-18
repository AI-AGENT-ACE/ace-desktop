import { useState, type ReactNode } from 'react';
import type { User } from '../../types';
import { AuthLayout } from './components/AuthLayout';
import { AuthForm } from './components/AuthForm';
import { useAuth } from './hooks/useAuth';

export function AuthGate({
  children,
}: {
  children: (user: User, logout: () => Promise<void>) => ReactNode;
}) {
  const auth = useAuth();
  const [mode, setMode] = useState<'login' | 'signup'>('login');
  const [notice, setNotice] = useState('');
  if (auth.state.status === 'authenticated') return children(auth.state.user, auth.logout);
  return (
    <AuthLayout onNotice={setNotice}>
      {auth.state.status === 'checking' ? (
        <div className="auth-splash" role="status">
          <img src="/ace-logo.png" alt="ACE" />
          <span className="auth-loading" aria-hidden="true" />
          <p>로그인 확인 중…</p>
        </div>
      ) : (
        <AuthForm
          key={mode}
          mode={mode}
          error={auth.error}
          busy={auth.busy}
          onSubmit={auth.authenticate}
          onSwitch={() => {
            setMode(mode === 'login' ? 'signup' : 'login');
            auth.clearError();
            setNotice('');
          }}
        />
      )}
      {notice && (
        <p className="auth-window-notice" role="alert">
          {notice}
        </p>
      )}
    </AuthLayout>
  );
}
