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
}) {
  const [text, setText] = useState('');
  const ref = useRef<HTMLTextAreaElement>(null);
  const submitting = useRef(false);
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
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
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
