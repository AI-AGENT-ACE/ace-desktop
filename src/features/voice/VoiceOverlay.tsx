import { useRef, type PointerEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { motion, useReducedMotion, useSpring, useTransform, type MotionValue } from 'motion/react';
import { X } from 'lucide-react';
import { type OrbVisualState } from './orbMotion';
import type { VoiceChoice } from './voice-session-policy';
const entrance = { opacity: [0, 1, 0.7, 1], scale: [0.94, 1.04, 1, 1] };
const still = { opacity: 1, scale: 1 };

const labels: Record<OrbVisualState, string> = {
  HIDDEN: '',
  IDLE: '준비됨',
  LISTENING: '듣고 있어요',
  TRANSCRIBING: '듣는 중…',
  THINKING: '생각하는 중…',
  EXECUTING: '실행 중…',
  SPEAKING: '답하는 중…',
  SUCCESS: '완료했어요',
  ERROR: '다시 시도해 주세요',
  WAITING_CONFIRMATION: '확인이 필요해요',
};

export function VoiceOverlay({
  visualState,
  transcript,
  audioLevel,
  recording,
  onStopRecording,
  onClose,
  choices = [],
  onChoice,
}: {
  visualState: OrbVisualState;
  transcript: string;
  audioLevel: MotionValue<number>;
  recording: boolean;
  onStopRecording: () => void;
  onClose: () => void;
  choices?: VoiceChoice[];
  onChoice?: (choice: string) => void;
}) {
  const reduced = useReducedMotion();
  const pointer = useRef<{ x: number; y: number; dragging: boolean } | null>(null);
  const smoothLevel = useSpring(audioLevel, { stiffness: 170, damping: 28, mass: 0.35 });
  const glowOpacity = useTransform(smoothLevel, [0, 1], [0.5, 0.65]);

  const beginPointer = (event: PointerEvent) => {
    if (event.button !== 0) return;
    pointer.current = { x: event.clientX, y: event.clientY, dragging: false };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const movePointer = (event: PointerEvent) => {
    const start = pointer.current;
    if (
      !start ||
      start.dragging ||
      Math.hypot(event.clientX - start.x, event.clientY - start.y) < 5
    )
      return;
    start.dragging = true;
    void invoke('drag_voice_orb').catch(() => undefined);
  };
  const endPointer = () => {
    const dragged = pointer.current?.dragging;
    pointer.current = null;
    if (!dragged && recording) onStopRecording();
  };

  const stateText = transcript.trim() || labels[visualState];
  return (
    <motion.aside
      className={`voice-orb-overlay is-${visualState.toLowerCase()}`}
      aria-label="ACE 음성 명령"
      aria-live="polite"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: reduced ? 0 : 0.18 }}
    >
      <button
        type="button"
        className="voice-orb-close"
        aria-label="음성 입력 닫기"
        onPointerDown={(event) => event.stopPropagation()}
        onClick={onClose}
      >
        <X size={14} />
      </button>
      <motion.div
        className="voice-orb-drag-handle"
        role="button"
        tabIndex={0}
        aria-label={
          recording
            ? 'ACE Orb, 클릭하여 녹음 종료 또는 드래그하여 이동'
            : 'ACE Orb, 드래그하여 이동'
        }
        initial={reduced ? false : { opacity: 0, scale: 0.94 }}
        animate={reduced ? still : entrance}
        transition={{ duration: reduced ? 0 : 0.48, times: [0, 0.35, 0.65, 1] }}
        onPointerDown={beginPointer}
        onPointerMove={movePointer}
        onPointerUp={endPointer}
        onPointerCancel={() => {
          pointer.current = null;
        }}
        onKeyDown={(event) => {
          if ((event.key === 'Enter' || event.key === ' ') && recording) onStopRecording();
        }}
      >
        <motion.span className="voice-orb-glow" style={{ opacity: glowOpacity }} />
        <span className="voice-orb-ring" />
        <span className="voice-orb-reactive" />
        <span className="voice-orb-core">
          <span className="voice-orb-core-light" />
        </span>
      </motion.div>
      <div className="voice-orb-bubble" role="status">
        <p className="voice-orb-transcript">{stateText}</p>
        {choices.length > 0 && (
          <div className="voice-orb-choices" aria-label="음성 명령 선택지">
            {choices.map((choice, index) => (
              <button key={choice.id} onClick={() => onChoice?.(choice.id)}>
                {choice.id === 'yes'
                  ? '네'
                  : choice.id === 'no'
                    ? '아니오'
                    : `${index + 1}. ${choice.label}`}
              </button>
            ))}
            {!choices.some((choice) => choice.id === 'no') && (
              <button onClick={() => onChoice?.('cancel')}>취소</button>
            )}
            <small>네·아니오 또는 번호로 답하거나 눌러 주세요.</small>
          </div>
        )}
      </div>
    </motion.aside>
  );
}
