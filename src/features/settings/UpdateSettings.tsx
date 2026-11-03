import { useEffect, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
type Status = {
  phase: string;
  version?: string;
  notes?: string;
  downloaded: number;
  total?: number;
  message: string;
};
export function UpdateSettings() {
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    if (import.meta.env.DEV || !isTauri()) return;
    let alive = true;
    const poll = () => {
      void invoke<Status>('update_status')
        .then((s) => {
          if (alive) setStatus(s);
        })
        .catch(() => {
          if (alive) setError('업데이트 상태를 확인하지 못했습니다.');
        });
    };
    poll();
    const timer = setInterval(poll, 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);
  const run = async (command: string) => {
    if (import.meta.env.DEV || !isTauri() || busy) return;
    setBusy(true);
    setError('');
    try {
      await invoke(command);
      setStatus(await invoke<Status>('update_status'));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };
  const active =
    busy || ['checking', 'downloading', 'waiting', 'installing'].includes(status?.phase || '');
  return (
    <section className="profile-editor">
      <h3>앱 업데이트</h3>
      {import.meta.env.DEV && <p role="status">개발 모드에서는 앱 업데이트를 사용할 수 없습니다.</p>}
      <button disabled={import.meta.env.DEV || !isTauri() || active} onClick={() => void run('check_app_update')}>
        업데이트 확인
      </button>
      {status?.version && <strong>새 버전 {status.version}</strong>}
      {status?.notes && <p style={{ whiteSpace: 'pre-wrap' }}>{status.notes}</p>}
      {status?.message && <p role="status">{status.message}</p>}
      {status?.phase === 'downloading' && (
        <>
          <progress
            aria-label="업데이트 다운로드"
            max={status.total || undefined}
            value={status.total ? status.downloaded : undefined}
          />
          <span>
            {Math.round(status.downloaded / 1024)} KB
            {status.total ? ` / ${Math.round(status.total / 1024)} KB` : ''}
          </span>
        </>
      )}
      {status?.phase === 'available' && (
        <button disabled={active} onClick={() => void run('install_app_update')}>
          다운로드 및 작업 종료 후 설치
        </button>
      )}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
