import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ArrowUp, FileAudio, FileText, Image, Mic, Plus, Paperclip, X } from 'lucide-react';

export type UploadProgress = { index: number; status: 'uploading' | 'uploaded' | 'failed'; progress: number; error?: string };
type SelectedFile = { file: File; preview?: string; status: 'pending' | 'uploading' | 'uploaded' | 'failed'; progress: number; error?: string };
const allowed = /\.(txt|md|csv|json|png|jpe?g|webp|pdf|docx|wav|mp3|m4a)$/i;
const maximum = (name: string) => /\.(pdf|docx)$/i.test(name) ? 25 * 1024 * 1024 : /\.(wav|mp3|m4a)$/i.test(name) ? 20 * 1024 * 1024 : 10 * 1024 * 1024;
const sizeLabel = (size: number) => size < 1024 * 1024 ? `${Math.ceil(size / 1024)}KB` : `${(size / 1024 / 1024).toFixed(1)}MB`;

export function ChatComposer({ onSend, onVoice, busy, wake }: {
  onSend: (text: string, files: File[], onProgress: (progress: UploadProgress) => void) => Promise<boolean>;
  onVoice: () => void;
  busy: boolean;
  wake: boolean;
}) {
  const [text, setText] = useState('');
  const [menu, setMenu] = useState(false);
  const [files, setFiles] = useState<SelectedFile[]>([]);
  const [fileError, setFileError] = useState('');
  const fileInput = useRef<HTMLInputElement>(null);
  const ref = useRef<HTMLTextAreaElement>(null);
  const filesRef = useRef(files);
  filesRef.current = files;
  useEffect(() => () => filesRef.current.forEach((item) => item.preview && URL.revokeObjectURL(item.preview)), []);
  useLayoutEffect(() => {
    if (ref.current) {
      ref.current.style.height = 'auto';
      ref.current.style.height = `${Math.min(ref.current.scrollHeight, 160)}px`;
    }
  }, [text]);
  const updateProgress = (progress: UploadProgress) => setFiles((items) => items.map((item, index) => index === progress.index ? { ...item, ...progress } : item));
  const send = async () => {
    if ((!text.trim() && !files.length) || busy) return;
    setFileError('');
    if (await onSend(text.trim(), files.map((item) => item.file), updateProgress)) {
      files.forEach((item) => item.preview && URL.revokeObjectURL(item.preview));
      setText('');
      setFiles([]);
    }
  };
  const addFiles = (selected: File[]) => {
    setFileError('');
    if (files.length + selected.length > 5) return setFileError('한 메시지에는 파일을 최대 5개까지 첨부할 수 있습니다.');
    if (selected.some((file) => !allowed.test(file.name))) return setFileError('지원 형식: TXT, MD, CSV, JSON, PNG, JPG, WEBP, PDF, DOCX, WAV, MP3, M4A');
    if (selected.some((file) => file.size === 0 || file.size > maximum(file.name))) return setFileError('파일 종류별 크기 제한을 확인해 주세요. 이미지·텍스트 10MB, 오디오 20MB, 문서 25MB입니다.');
    if ([...files.map((item) => item.file), ...selected].reduce((sum, file) => sum + file.size, 0) > 50 * 1024 * 1024) return setFileError('첨부 파일의 전체 크기는 50MB 이하여야 합니다.');
    setFiles((items) => [...items, ...selected.map((file) => ({ file, preview: file.type.startsWith('image/') ? URL.createObjectURL(file) : undefined, status: 'pending' as const, progress: 0 }))]);
    ref.current?.focus();
  };
  return (
    <div className="composer-area"><div className="chat-content"><div className="composer">
      <input ref={fileInput} type="file" aria-label="첨부 파일 선택" accept=".txt,.md,.csv,.json,.png,.jpg,.jpeg,.webp,.pdf,.docx,.wav,.mp3,.m4a" multiple hidden onChange={(event) => { const selected = Array.from(event.target.files || []); event.target.value = ''; addFiles(selected); }} />
      {!!files.length && <div className="composer-attachments">{files.map((item, index) => (
        <div className={`composer-attachment ${item.status}`} key={`${item.file.name}-${item.file.lastModified}`}>
          {item.preview ? <img src={item.preview} alt="" /> : item.file.type.startsWith('audio/') ? <FileAudio size={18} /> : <FileText size={18} />}
          <span><strong title={item.file.name}>{item.file.name}</strong><small>{sizeLabel(item.file.size)} · {item.status === 'pending' ? '대기 중' : item.status === 'uploading' ? `업로드 ${item.progress}%` : item.status === 'uploaded' ? '업로드 완료' : item.error || '실패'}</small></span>
          <button className="icon-button" aria-label={`${item.file.name} 제거`} disabled={busy} onClick={() => setFiles((items) => { const target = items[index]; if (target.preview) URL.revokeObjectURL(target.preview); return items.filter((_, itemIndex) => itemIndex !== index); })}><X size={14} /></button>
        </div>
      ))}</div>}
      {fileError && <p className="composer-file-error" role="alert">{fileError}</p>}
      <textarea ref={ref} aria-label="메시지" placeholder="ACE에게 무엇이든 물어보세요" value={text} disabled={busy} rows={1} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); void send(); } }} />
      <div className="composer-tools"><div className="tool-anchor"><button className="icon-button" aria-label="추가 기능" aria-expanded={menu} disabled={busy} onClick={() => setMenu(!menu)}><Plus size={21} /></button>{menu && <div className="tool-menu"><button onClick={() => { setMenu(false); fileInput.current?.click(); }}><Paperclip size={16} />파일 추가</button><p className="composer-file-status"><Image size={13} /> 이미지 · 문서 · 오디오 · 텍스트</p></div>}</div>
        <span className="composer-hint">Shift + Enter로 줄바꿈</span><button className="icon-button" aria-label="음성 입력" onClick={onVoice} disabled={busy}><Mic size={19} /></button><button className="send-button" aria-label="메시지 보내기" disabled={(!text.trim() && !files.length) || busy} onClick={() => void send()}><ArrowUp size={21} /></button>
      </div>
    </div><footer className="composer-footer"><span><i className={wake ? 'status-dot' : 'status-dot off'} />{wake ? 'Wake Word 설정 켜짐 · 감지 미연결' : 'Wake Word 꺼짐'}</span></footer></div></div>
  );
}
