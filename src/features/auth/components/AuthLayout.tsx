import { useLayoutEffect, type ReactNode } from 'react';
import { CustomTitleBar } from '../../../layouts/CustomTitleBar';

export function AuthLayout({
  children,
  onNotice,
}: {
  children: ReactNode;
  onNotice: (message: string) => void;
}) {
  useLayoutEffect(() => {
    document.documentElement.dataset.theme =
      localStorage.getItem('ace-theme') === 'dark' ? 'dark' : 'light';
  }, []);
  return (
    <div className="app auth-app">
      <CustomTitleBar onNotice={onNotice} />
      <main className="auth-screen">
        <div className="auth-content">{children}</div>
      </main>
      <footer className="auth-footer">ACE Auto Computer Executor</footer>
    </div>
  );
}
