import { useEffect, useState, type MouseEvent } from 'react';
import { Copy, Minus, Square, X } from 'lucide-react';
import { windowAdapter } from '../lib/windowAdapter';
export function CustomTitleBar({ onNotice }: { onNotice: (message: string) => void }) {
  const [maximized, setMaximized] = useState(false);
  const [animating, setAnimating] = useState(false);
  useEffect(() => {
    if (!windowAdapter.available()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const refresh = async () => { const value = await windowAdapter.isMaximized(); if (!disposed) setMaximized(value); };
    void (async () => {
      try {
        const stop = await windowAdapter.onResize(() => { void refresh().catch(console.error); });
        if (disposed) { stop(); return; }
        unlisten = stop;
        await refresh();
      } catch (error) { console.error('Window state initialization failed', error); }
    })();
    return () => { disposed = true; unlisten?.(); };
  }, []);
  const run = async (operation: () => Promise<void>) => {
    if (animating) return;
    setAnimating(true);
    try { await operation(); } catch (error) { onNotice(error instanceof Error ? error.message : `창 제어에 실패했습니다: ${String(error)}`); }
    finally { setAnimating(false); }
  };
  const toggle = () => run(async () => { await windowAdapter.toggleMaximize(); setMaximized(await windowAdapter.isMaximized()); });
  const close = async () => { try { await windowAdapter.close(); } catch (error) { onNotice(error instanceof Error ? error.message : String(error)); } };
  const drag = (event: MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0 || animating || !windowAdapter.available()) return;
    event.preventDefault();
    if (event.detail === 2) void toggle();
    else void run(windowAdapter.startDragging);
  };
  return <header className="titlebar"><div className="titlebar-brand" onMouseDown={drag}><img src="/ace-logo.png" alt="" draggable={false}/><span>ACE</span></div><div className="drag-region" onMouseDown={drag}/><div className="window-controls"><button aria-label="창 최소화" title="최소화" onClick={() => void run(windowAdapter.minimize)}><Minus size={15}/></button><button aria-label={maximized ? '창 복원' : '창 최대화'} title={maximized ? '복원' : '최대화'} onClick={() => void toggle()}>{maximized ? <Copy size={12}/> : <Square size={12}/>}</button><button className="close-window" aria-label="창 닫기" title="닫기 · 트레이에서 ACE 열기" onClick={() => void close()}><X size={16}/></button></div></header>;
}
