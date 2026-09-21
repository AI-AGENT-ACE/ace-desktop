import { AudioLines, X, AlertCircle, LoaderCircle } from 'lucide-react';
import type { VoiceState } from '../../types';
export function VoiceOverlay({
  state,
  transcript,
  onTranscript,
  onFinish,
  onClose,
  recording,
  recordingPath,
  onStopRecording,
}: {
  state: VoiceState;
  transcript: string;
  onTranscript: (text: string) => void;
  onFinish: () => void;
  onClose: () => void;
  recording?: boolean;
  recordingPath?: string;
  onStopRecording?: () => void;
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
        STT는 아직 연결되지 않았습니다. 마이크 음성은 로컬 WAV 파일로 저장할 수 있으며, 명령 문장은
        직접 입력해야 합니다.
      </p>
      {onStopRecording && (
        <div className="voice-recording-status">
          <span>
            {recording ? '마이크 녹음 중' : recordingPath ? 'WAV 저장 완료' : '녹음 준비 중'}
          </span>
          {recording && (
            <button type="button" onClick={onStopRecording} disabled={state === 'processing'}>
              녹음 종료 · WAV 저장
            </button>
          )}
          {recordingPath && <small title={recordingPath}>{recordingPath}</small>}
        </div>
      )}
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
