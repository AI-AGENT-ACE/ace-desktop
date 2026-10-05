import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';

type Sensitivity = 'standard' | 'sensitive';
export function WakeSensitivitySettings() {
  const [value, setValue] = useState<Sensitivity>('standard');
  const [ready, setReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const locked = useRef(false);
  useEffect(() => {
    let mounted = true;
    if (isTauri()) {
      void invoke<{ settings: { sensitivity?: Sensitivity } }>('get_wake_word_setup_status')
        .then(({ settings }) => {
          if (mounted) {
            setValue(settings.sensitivity ?? 'standard');
            setReady(true);
          }
        })
        .catch(() => {
          if (mounted) setMessage('호출 감도 설정을 읽지 못했습니다.');
        });
    }
    return () => {
      mounted = false;
    };
  }, []);
  if (!isTauri()) return null;
  const change = async (sensitivity: Sensitivity) => {
    if (locked.current) return;
    locked.current = true;
    setBusy(true);
    setMessage('');
    try {
      const saved = await invoke<{ sensitivity: Sensitivity }>('set_wake_word_sensitivity', {
        sensitivity,
      });
      setValue(saved.sensitivity);
      setMessage('호출 감도를 적용했습니다. “ACE”라고 불러보세요.');
    } catch (cause) {
      setMessage(typeof cause === 'string' ? cause : '호출 감도를 적용하지 못했습니다.');
    } finally {
      locked.current = false;
      setBusy(false);
    }
  };
  return (
    <div className="wake-settings-detail">
      <label htmlFor="wake-sensitivity">호출 감도</label>
      <select
        id="wake-sensitivity"
        value={value}
        disabled={!ready || busy}
        onChange={(event) => void change(event.target.value as Sensitivity)}
      >
        <option value="standard">기본</option>
        <option value="sensitive">민감하게 — 여러 번 불러야 할 때</option>
      </select>
      <p>
        잘 반응하지 않으면 ‘민감하게’를 선택하세요. 다른 말에도 반응하면 ‘기본’으로 되돌릴 수
        있습니다.
      </p>
      {message && <p role="status">{message}</p>}
    </div>
  );
}
