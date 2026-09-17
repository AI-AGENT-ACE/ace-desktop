import { useLayoutEffect, useRef, useState } from 'react';
import { ArrowUp, Mic, Plus, Volume2, Monitor, Music2 } from 'lucide-react';
export function ChatComposer({
  onSend,
  onVoice,
  busy,
  wake,
}: {
  onSend: (text: string) => Promise<boolean>;
  onVoice: () => void;
  busy: boolean;
  wake: boolean;
}) {
  const [text, setText] = useState('');
  const [menu, setMenu] = useState(false);
  const ref = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    if (ref.current) {
      ref.current.style.height = 'auto';
      ref.current.style.height = `${Math.min(ref.current.scrollHeight, 160)}px`;
    }
  }, [text]);
  const send = async () => {
    if (!text.trim() || busy) return;
    if (await onSend(text.trim())) setText('');
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
            disabled={busy}
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
            <div className="tool-anchor">
              <button
                className="icon-button"
                aria-label="추가 기능"
                aria-expanded={menu}
                onClick={() => setMenu(!menu)}
              >
                <Plus size={21} />
              </button>
              {menu && (
                <div
                  className="tool-menu"
                  onKeyDown={(e) => {
                    if (e.key === 'Escape') setMenu(false);
                  }}
                >
                  {[
                    [Volume2, '현재 볼륨 조회'],
                    [Monitor, '시스템 상태 조회'],
                    [Music2, 'Spotify 실행'],
                  ].map(([Icon, label]) => {
                    const I = Icon as typeof Volume2;
                    return (
                      <button
                        key={String(label)}
                        onClick={() => {
                          setText(String(label));
                          setMenu(false);
                          ref.current?.focus();
                        }}
                      >
                        <I size={16} />
                        {String(label)}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
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
              className="send-button"
              aria-label="메시지 보내기"
              disabled={!text.trim() || busy}
              onClick={() => void send()}
            >
              <ArrowUp size={21} />
            </button>
          </div>
        </div>
        <footer className="composer-footer">
          <span>
            <i className={wake ? 'status-dot' : 'status-dot off'} />
            {wake ? 'Wake Word 설정 켜짐 · 감지 미연결' : 'Wake Word 꺼짐'}
          </span>
          <span>ACE의 답변은 중요한 정보의 경우 확인해 주세요.</span>
        </footer>
      </div>
    </div>
  );
}
