import { AudioLines, X, AlertCircle, LoaderCircle } from 'lucide-react';
import type { VoiceState } from '../../types';
export function VoiceOverlay({
  state,
  transcript,
  onTranscript,
  onFinish,
  onClose,
}: {
  state: VoiceState;
  transcript: string;
  onTranscript: (text: string) => void;
  onFinish: () => void;
  onClose: () => void;
}) {
  return (
    <aside className="voice-overlay" aria-label="음성 명령 입력" aria-live="polite">
      <div className="voice-heading">
        {state === 'processing' ? (
          <LoaderCircle className="spin" size={20} />
        ) : state === 'error' ? (
          <AlertCircle size={20} />
        ) : (
          <AudioLines size={22} />
        )}
        <strong>
          {state === 'listening'
            ? 'ACE가 듣는 중'
            : state === 'processing'
              ? '음성 인식 중…'
              : '음성 입력 오류'}
        </strong>
        <button className="icon-button" aria-label="음성 입력 닫기" onClick={onClose}>
          <X size={16} />
        </button>
      </div>
      <p>
        STT는 아직 연결되지 않았습니다. ‘시스템 상태 조회’, ‘메모장 실행’ 등의 명령을 입력하세요.
        원문은 대화에 저장하지 않습니다.
      </p>
      <input
        aria-label="음성 명령 문장"
        maxLength={200}
        placeholder="예: 메모장 실행"
        value={transcript}
        onChange={(e) => onTranscript(e.target.value)}
        disabled={state === 'processing'}
        onKeyDown={(e) => {
          if (e.key === 'Enter') onFinish();
        }}
      />
      <button
        className="primary"
        disabled={!transcript.trim() || state === 'processing'}
        onClick={onFinish}
      >
        명령 실행
      </button>
    </aside>
  );
}
