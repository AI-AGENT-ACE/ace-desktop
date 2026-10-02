import { useEffect, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';

type CaptureStatus = { state: string; directory?: string; error?: string; summary?: unknown };
export function WakeDiagnosticPanel() {
  const [status, setStatus] = useState<CaptureStatus>({ state: 'idle' });
  const [label, setLabel] = useState('positive');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    if (!import.meta.env.DEV || !isTauri()) return;
    let disposed = false;
    const poll = () => void invoke<CaptureStatus>('get_wake_diagnostic').then(value => {
      if (!disposed) setStatus(value);
    }).catch(cause => { if (!disposed) setError(String(cause)); });
    poll();
    const timer = window.setInterval(poll, 500);
    return () => { disposed = true; window.clearInterval(timer); };
  }, []);
  if (!import.meta.env.DEV || !isTauri()) return null;
  const active = ['starting', 'recording', 'saving'].includes(status.state);
  const command = async (name: string) => {
    setBusy(true); setError('');
    try { await invoke(name, name === 'start_wake_diagnostic' ? { label } : undefined); }
    catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  };
  return <details className="wake-settings-detail">
    <summary>개발용 Wake Word 진단 녹음</summary>
    <p>직접 시작한 경우에만 엔진 입력을 최대 30초간 로컬에 저장합니다. 업로드하지 않습니다. 녹음 중에는 Orb가 열리지 않습니다.</p>
    <p>‘recording’ 표시 후 침묵 3초 → ACE 5회(각 2~3초 간격) → 침묵 3초 → 종료 순서로 진행해 주세요.</p>
    <select aria-label="진단 녹음 종류" value={label} disabled={active || busy} onChange={e => setLabel(e.target.value)}>
      <option value="positive">ACE 5회</option><option value="silence">침묵</option><option value="negative">ACE 없는 일반 발화</option>
    </select>
    <div className="modal-actions">
      <button disabled={active || busy} onClick={() => void command('start_wake_diagnostic')}>진단 녹음 시작</button>
      <button disabled={!active || busy || status.state === 'saving'} onClick={() => void command('stop_wake_diagnostic')}>종료 및 저장</button>
    </div>
    <p role="status">상태: {status.state}</p>
    {status.directory && <p style={{ overflowWrap: 'anywhere' }}>저장 폴더: {status.directory}</p>}
    {(error || status.error) && <p role="alert">{error || status.error}</p>}
  </details>;
}
