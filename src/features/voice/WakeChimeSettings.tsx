import { useState } from 'react';
import { playWakeChime, wakeChimeEnabled } from './voice-session-policy';

export function WakeChimeSettings() {
  const [enabled, setEnabled] = useState(wakeChimeEnabled);
  return (
    <div className="setting-row">
      <div>
        <strong>음성 호출 알림음</strong>
        <p>오브를 부를 때 짧게 소리를 냅니다. 이 PC에 저장됩니다.</p>
      </div>
      <button
        className={`toggle ${enabled ? 'on' : ''}`}
        role="switch"
        aria-checked={enabled}
        aria-label="음성 호출 알림음"
        onClick={() => {
          const next = !enabled;
          localStorage.setItem('ace-wake-chime', next ? 'on' : 'off');
          setEnabled(next);
          if (next) void playWakeChime().catch(() => undefined);
        }}
      >
        <span />
      </button>
    </div>
  );
}
