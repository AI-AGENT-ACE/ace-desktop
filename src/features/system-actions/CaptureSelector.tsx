import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
type Context = {
  sessionId: string;
  name: string;
  mode: 'full' | 'region';
  width: number;
  height: number;
  scaleFactor: number;
};
type Region = { x: number; y: number; width: number; height: number };
export function CaptureSelector() {
  const [context, setContext] = useState<Context | null>(null);
  const [region, setRegion] = useState<Region | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const anchor = useRef<{ x: number; y: number } | null>(null);
  const lock = useRef(false);
  const id = useRef(new URLSearchParams(location.hash.split('?')[1]).get('session'));
  const cancel = () => {
    if (!lock.current && id.current)
      void invoke('cancel_screen_capture', { sessionId: id.current });
  };
  useEffect(() => {
    document.documentElement.classList.add('capture-document');
    let active = true;
    void invoke<Context>('get_capture_context')
      .then((c) => {
        if (active) {
          id.current = c.sessionId;
          setContext(c);
        }
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        cancel();
      }
    };
    window.addEventListener('keydown', key);
    return () => {
      active = false;
      window.removeEventListener('keydown', key);
      document.documentElement.classList.remove('capture-document');
    };
  }, []);
  const point = (event: React.PointerEvent) => ({
    x: Math.max(0, Math.min(window.innerWidth, event.clientX)),
    y: Math.max(0, Math.min(window.innerHeight, event.clientY)),
  });
  const finish = async () => {
    if (lock.current || !context || (context.mode === 'region' && !region)) return;
    lock.current = true;
    setBusy(true);
    try {
      await invoke('complete_screen_capture', {
        sessionId: context.sessionId,
        region: context.mode === 'region' ? region : null,
      });
    } catch (e) {
      setError(String(e));
      lock.current = false;
      setBusy(false);
    }
  };
  useEffect(() => {
    if (context?.mode === 'full') void finish();
    // A full capture is already selected/authorized. The ref prevents duplicate IPC.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [context]);
  return (
    <div
      className="capture-selector"
      onPointerDown={(e) => {
        if (
          e.button !== 0 ||
          busy ||
          context?.mode !== 'region' ||
          (e.target as HTMLElement).closest('.capture-controls')
        )
          return;
        anchor.current = point(e);
        e.currentTarget.setPointerCapture(e.pointerId);
        setRegion(null);
      }}
      onPointerMove={(e) => {
        if (!anchor.current) return;
        const p = point(e),
          a = anchor.current;
        setRegion({
          x: Math.min(a.x, p.x),
          y: Math.min(a.y, p.y),
          width: Math.abs(p.x - a.x),
          height: Math.abs(p.y - a.y),
        });
      }}
      onPointerUp={(e) => {
        anchor.current = null;
        if (e.currentTarget.hasPointerCapture(e.pointerId))
          e.currentTarget.releasePointerCapture(e.pointerId);
      }}
    >
      {region && (
        <div
          className="capture-region"
          style={{ left: region.x, top: region.y, width: region.width, height: region.height }}
        />
      )}
      <div className="capture-controls" role="dialog" aria-label="화면 캡처 선택">
        <strong>{context?.name || '화면 캡처'}</strong>
        <span>
          {context?.mode === 'region'
            ? '이 모니터 안에서 영역을 드래그하세요.'
            : '이 모니터 전체를 캡처합니다.'}{' '}
          Esc로 취소
        </span>
        {error && <p role="alert">{error}</p>}
        <button
          disabled={
            busy ||
            !context ||
            (context.mode === 'region' && (!region || region.width < 1 || region.height < 1))
          }
          onClick={() => void finish()}
        >
          캡처
        </button>
        <button disabled={busy} onClick={cancel}>
          취소
        </button>
      </div>
    </div>
  );
}
