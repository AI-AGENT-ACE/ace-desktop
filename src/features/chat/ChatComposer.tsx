import { useLayoutEffect, useRef, useState } from 'react';
import { ArrowUp, Square, Mic } from 'lucide-react';

export type UploadProgress = {
  index: number;
  status: 'uploading' | 'uploaded' | 'failed';
  progress: number;
  error?: string;
};
export function ChatComposer({
  onSend,
  onVoice,
  busy,
  sending,
  onStop,
  history = [],
  hasOlderHistory = false,
  historyLoading = false,
  onLoadOlderHistory,
}: {
  onSend: (
    text: string,
    files: File[],
    onProgress: (progress: UploadProgress) => void,
  ) => Promise<boolean>;
  onVoice: () => void;
  busy: boolean;
  sending?: boolean;
  onStop?: () => void;
  history?: string[];
  hasOlderHistory?: boolean;
  historyLoading?: boolean;
  onLoadOlderHistory?: () => Promise<void>;
}) {
  const [text, setText] = useState('');
  const ref = useRef<HTMLTextAreaElement>(null);
  const submitting = useRef(false);
  const historyIndex = useRef<number | null>(null);
  const historySnapshot = useRef<string[]>([]);
  const waitingForHistory = useRef(false);
  useLayoutEffect(() => {
    if (!waitingForHistory.current || historyLoading) return;
    const added = history.length - historySnapshot.current.length;
    if (added > 0) {
      historySnapshot.current = [...history];
      historyIndex.current = added - 1;
      setText(history[added - 1]);
      waitingForHistory.current = false;
    } else waitingForHistory.current = false;
  }, [history, historyLoading, hasOlderHistory]);
  useLayoutEffect(() => {
    if (ref.current) {
      ref.current.style.height = 'auto';
      ref.current.style.height = `${Math.min(ref.current.scrollHeight, 160)}px`;
    }
  }, [text]);
  const send = async () => {
    if (!text.trim() || busy || submitting.current) return;
    submitting.current = true;
    const submittedText = text.trim();
    setText('');
    historyIndex.current = null;
    waitingForHistory.current = false;
    try {
      if (!(await onSend(submittedText, [], () => {})) && !ref.current?.value) {
        setText(submittedText);
      }
    } finally {
      submitting.current = false;
    }
  };
  return (
    <div className="composer-area">
      <div className="chat-content">
        <div className="composer">
          <textarea
            ref={ref}
            aria-label="메시지"
            placeholder="ACE에게 무엇이든 물어보세요"
            value={text}
            rows={1}
            onChange={(e) => {
              historyIndex.current = null;
              waitingForHistory.current = false;
              setText(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey)
                return;
              if (e.key === 'ArrowUp' && (text === '' || historyIndex.current !== null)) {
                if (historyIndex.current === 0 && hasOlderHistory && onLoadOlderHistory) {
                  e.preventDefault();
                  if (!historyLoading && !waitingForHistory.current) {
                    waitingForHistory.current = true;
                    void onLoadOlderHistory().catch(() => {
                      waitingForHistory.current = false;
                    });
                  }
                  return;
                }
                if (historyIndex.current === null) historySnapshot.current = [...history];
                if (!historySnapshot.current.length) return;
                e.preventDefault();
                const next =
                  historyIndex.current === null
                    ? historySnapshot.current.length - 1
                    : Math.max(0, historyIndex.current - 1);
                historyIndex.current = next;
                setText(historySnapshot.current[next]);
                return;
              }
              if (e.key === 'ArrowDown' && historyIndex.current !== null) {
                e.preventDefault();
                const next = historyIndex.current + 1;
                historyIndex.current = next < historySnapshot.current.length ? next : null;
                setText(historySnapshot.current[next] ?? '');
                return;
              }
              if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                void send();
              }
            }}
          />
          <div className="composer-tools">
            <span className="composer-hint">Shift + Enter로 줄바꿈</span>
            <button
              className="icon-button"
              aria-label="음성 입력"
              onClick={onVoice}
              disabled={busy}
            >
              <Mic size={19} />
            </button>
            <button
              className={`send-button ${sending ? 'stop-button' : ''}`}
              aria-label={sending ? '답변 생성 중지' : '메시지 보내기'}
              disabled={!sending && (!text.trim() || busy)}
              onClick={() => (sending ? onStop?.() : void send())}
            >
              {sending ? <Square size={16} fill="currentColor" /> : <ArrowUp size={21} />}
            </button>
          </div>
        </div>
        <footer className="composer-footer" aria-hidden="true" />
      </div>
    </div>
  );
}
