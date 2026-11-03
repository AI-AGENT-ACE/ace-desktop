import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
type Preference = { deviceId: string | null; allowDefaultFallback: boolean };
type Status = {
  settings: Preference;
  devices: { id: string; name: string }[];
  currentDeviceId: string | null;
  currentDeviceName: string | null;
};
export function MicrophoneSettings() {
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [level, setLevel] = useState(0);
  const mounted = useRef(true);
  const lock = useRef(false);
  const session = useRef<string | null>(null);
  const refresh = () =>
    invoke<Status>('get_audio_input_status').then((value) => {
      if (mounted.current) setStatus(value);
    });
  const stopTest = async () => {
    session.current = null;
    await invoke('stop_audio_input');
    if (mounted.current) {
      setTesting(false);
      setLevel(0);
    }
  };
  useEffect(() => {
    mounted.current = true;
    if (!isTauri()) return;
    const unsubscribers: (() => void)[] = [];
    let disposed = false;
    const register = async () => {
      const audio = await listen<{ sessionId: string; rms: number }>(
        'ace-audio-input',
        ({ payload }) => {
          if (mounted.current && payload.sessionId === session.current)
            setLevel(Math.min(1, payload.rms / 0.15));
        },
      );
      const errors = await listen<{ sessionId: string; message: string }>(
        'ace-audio-input-error',
        ({ payload }) => {
          if (mounted.current && payload.sessionId === session.current) {
            setError(payload.message);
            void stopTest().catch(() => undefined);
          }
        },
      );
      if (disposed) {
        audio();
        errors();
      } else unsubscribers.push(audio, errors);
    };
    void register();
    void refresh().catch((cause) => setError(String(cause)));
    const timer = setInterval(() => void refresh().catch(() => undefined), 1500);
    return () => {
      disposed = true;
      mounted.current = false;
      clearInterval(timer);
      unsubscribers.forEach((fn) => fn());
      if (session.current) void stopTest().catch(() => undefined);
    };
  }, []);
  const save = async (preference: Preference) => {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError('');
    try {
      if (session.current) await stopTest();
      const next = await invoke<Status>('set_audio_input', { preference });
      if (mounted.current) setStatus(next);
    } catch (cause) {
      if (mounted.current) {
        setError(String(cause));
        await refresh().catch(() => undefined);
      }
    } finally {
      lock.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  const test = async () => {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError('');
    try {
      if (testing) await stopTest();
      else {
        const id = crypto.randomUUID();
        session.current = id;
        await invoke('start_audio_input', { sessionId: id, monitor: true });
        if (mounted.current) setTesting(true);
        else await stopTest();
      }
    } catch (cause) {
      session.current = null;
      if (mounted.current) setError(String(cause));
    } finally {
      lock.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  if (!isTauri()) return null;
  return (
    <>
      <div className="setting-row">
        <div>
          <strong>입력 마이크</strong>
          <p>음성 명령과 ACE 호출어 감지에 같은 마이크를 사용합니다.</p>
        </div>
        <select
          aria-label="입력 마이크"
          value={status?.settings.deviceId ?? ''}
          disabled={!status || busy}
          onChange={(e) =>
            status && void save({ ...status.settings, deviceId: e.target.value || null })
          }
        >
          <option value="">시스템 기본 마이크</option>
          {status?.settings.deviceId &&
            !status.devices.some((d) => d.id === status.settings.deviceId) && (
              <option value={status.settings.deviceId}>연결되지 않은 마이크</option>
            )}
          {status?.devices.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name}
            </option>
          ))}
        </select>
      </div>
      <div className="setting-row">
        <div>
          <strong>기본 마이크로 자동 대체</strong>
          <p>선택 장치가 없으면 다음 입력 시작 시 시스템 기본 마이크를 사용합니다.</p>
        </div>
        <input
          type="checkbox"
          aria-label="기본 마이크로 자동 대체"
          checked={status?.settings.allowDefaultFallback ?? false}
          disabled={!status || busy}
          onChange={(e) =>
            status && void save({ ...status.settings, allowDefaultFallback: e.target.checked })
          }
        />
      </div>
      <div className="setting-row">
        <div>
          <strong>마이크 입력 확인</strong>
          <p>{status?.currentDeviceName || '사용 가능한 마이크가 없습니다. 다시 선택해 주세요.'}</p>
          <meter aria-label="마이크 음량" min={0} max={1} value={level} />
        </div>
        <button disabled={!status || busy} onClick={() => void test()}>
          {testing ? '확인 종료' : '음량 확인'}
        </button>
      </div>
      {error && (
        <p className="danger-text" role="alert">
          {error}
        </p>
      )}
    </>
  );
}
