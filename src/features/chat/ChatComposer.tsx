import { useLayoutEffect, useRef, useState } from 'react';
import { ArrowUp, Mic, Plus, Paperclip, X } from 'lucide-react';
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
  const [file, setFile] = useState<{ name: string; content: string } | null>(null);
  const [fileError, setFileError] = useState('');
  const [reading, setReading] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const ref = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    if (ref.current) {
      ref.current.style.height = 'auto';
      ref.current.style.height = `${Math.min(ref.current.scrollHeight, 160)}px`;
    }
  }, [text]);
  const send = async () => {
    if ((!text.trim() && !file) || busy || reading) return;
    const message = file
      ? `${text.trim()}${text.trim() ? '\n\n' : ''}[첨부 파일: ${file.name}]\n${file.content}`
      : text.trim();
    if (message.length > 20000) {
      setFileError('메시지와 파일 내용을 합쳐 20,000자 이내로 보내 주세요.');
      return;
    }
    if (await onSend(message)) {
      setText('');
      setFile(null);
      setFileError('');
    }
  };
  const addFile = async (selected: File) => {
    setFileError('');
    if (!/\.(txt|md|csv|json)$/i.test(selected.name)) {
      setFileError('TXT, MD, CSV, JSON 텍스트 파일을 선택해 주세요.');
      return;
    }
    if (selected.size > 64 * 1024) {
      setFileError('64KB 이하의 텍스트 파일을 선택해 주세요.');
      return;
    }
    setReading(true);
    try {
      const content = await selected.text();
      if (!content.trim() || content.length > 15000 || /[\u0000\uFFFD]/.test(content)) {
        setFileError(
          '내용이 있는 UTF-8 텍스트 파일을 선택해 주세요. 최대 15,000자까지 지원합니다.',
        );
        return;
      }
      setFile({ name: selected.name, content });
      ref.current?.focus();
    } catch {
      setFileError('파일을 읽지 못했습니다. 다시 선택해 주세요.');
    } finally {
      setReading(false);
    }
  };
  return (
    <div className="composer-area">
      <div className="chat-content">
        <div className="composer">
          <input
            ref={fileInput}
            type="file"
            aria-label="첨부 파일 선택"
            accept=".txt,.md,.csv,.json"
            hidden
            onChange={(event) => {
              const selected = event.target.files?.[0];
              event.target.value = '';
              if (selected) void addFile(selected);
            }}
          />
          {file && (
            <div className="composer-attachment">
              <Paperclip size={15} />
              <span title={file.name}>{file.name}</span>
              <button
                className="icon-button"
                aria-label="첨부 파일 제거"
                disabled={busy || reading}
                onClick={() => {
                  setFile(null);
                  setFileError('');
                }}
              >
                <X size={14} />
              </button>
            </div>
          )}
          {reading && (
            <p className="composer-file-status" role="status">
              파일을 읽고 있습니다.
            </p>
          )}
          {fileError && (
            <p className="composer-file-error" role="alert">
              {fileError}
            </p>
          )}
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
                disabled={busy || reading}
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
                  <button
                    onClick={() => {
                      setMenu(false);
                      fileInput.current?.click();
                    }}
                  >
                    <Paperclip size={16} />
                    파일 추가
                  </button>
                  <p className="composer-file-status">TXT · MD · CSV · JSON</p>
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
              disabled={(!text.trim() && !file) || busy || reading}
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
        </footer>
      </div>
    </div>
  );
}
