import { useRef, type PointerEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  AnimatePresence,
  motion,
  useReducedMotion,
  useSpring,
  useTransform,
  type MotionValue,
} from 'motion/react';
import { X } from 'lucide-react';
import { orbVariants, ringVariants, textVariants, type OrbVisualState } from './orbMotion';

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
}: {
  visualState: OrbVisualState;
  transcript: string;
  audioLevel: MotionValue<number>;
  recording: boolean;
  onStopRecording: () => void;
  onClose: () => void;
}) {
  const reduced = useReducedMotion();
  const pointer = useRef<{ x: number; y: number; dragging: boolean } | null>(null);
  const smoothLevel = useSpring(audioLevel, { stiffness: 170, damping: 28, mass: 0.35 });
  const reactiveScale = useTransform(smoothLevel, [0, 1], [1, reduced ? 1.025 : 1.12]);
  const glowOpacity = useTransform(smoothLevel, [0, 1], [0.24, reduced ? 0.42 : 0.78]);

  const beginPointer = (event: PointerEvent) => {
    if (event.button !== 0) return;
    pointer.current = { x: event.clientX, y: event.clientY, dragging: false };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const movePointer = (event: PointerEvent) => {
    const start = pointer.current;
    if (!start || start.dragging || Math.hypot(event.clientX - start.x, event.clientY - start.y) < 5)
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
      initial="hidden"
      animate={visualState}
      exit="exit"
      variants={reduced ? undefined : orbVariants}
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
        aria-label={recording ? 'ACE Orb, 클릭하여 녹음 종료 또는 드래그하여 이동' : 'ACE Orb, 드래그하여 이동'}
        style={{ scale: visualState === 'LISTENING' ? reactiveScale : 1 }}
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
        <motion.span className="voice-orb-glow" style={{ opacity: visualState === 'LISTENING' ? glowOpacity : undefined }} />
        <motion.span className="voice-orb-ring" variants={reduced ? undefined : ringVariants} animate={visualState} />
        <span className="voice-orb-reactive" />
        <span className="voice-orb-core"><span className="voice-orb-core-light" /></span>
      </motion.div>
      <AnimatePresence mode="wait">
        <motion.p
          key={`${visualState}:${stateText}`}
          className="voice-orb-transcript"
          variants={reduced ? undefined : textVariants}
          initial="hidden"
          animate="visible"
          exit="exit"
          title={stateText}
        >
          {stateText}
        </motion.p>
      </AnimatePresence>
    </motion.aside>
  );
}
