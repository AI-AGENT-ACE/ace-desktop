import type { Variants } from 'motion/react';

export type OrbVisualState =
  | 'HIDDEN'
  | 'IDLE'
  | 'LISTENING'
  | 'TRANSCRIBING'
  | 'THINKING'
  | 'EXECUTING'
  | 'SPEAKING'
  | 'SUCCESS'
  | 'ERROR'
  | 'WAITING_CONFIRMATION';

export const orbVariants: Variants = {
  hidden: { opacity: 0, scale: 0.9 },
  HIDDEN: { opacity: 0, scale: 0.9, pointerEvents: 'none' },
  IDLE: { opacity: 1, scale: [1, 1.018, 1], transition: { duration: 3.8, repeat: Infinity } },
  LISTENING: { opacity: 1, scale: 1, transition: { type: 'spring', stiffness: 210, damping: 22 } },
  TRANSCRIBING: { opacity: 1, scale: [1, 1.025, 1], rotate: [0, 3, 0], transition: { duration: 2.2, repeat: Infinity } },
  THINKING: { opacity: 1, scale: [1, 1.045, 1], transition: { duration: 1.65, repeat: Infinity, ease: 'easeInOut' } },
  EXECUTING: { opacity: 1, scale: [1, 1.06, 1], transition: { duration: 0.9, repeat: Infinity, ease: 'easeInOut' } },
  SPEAKING: { opacity: 1, scale: [1, 1.055, 1.015, 1], transition: { duration: 1.15, repeat: Infinity } },
  SUCCESS: { opacity: 1, scale: [1, 1.12, 1], transition: { duration: 0.48, ease: 'easeOut' } },
  ERROR: { opacity: 1, x: [0, -5, 5, -3, 3, 0], transition: { duration: 0.42 } },
  WAITING_CONFIRMATION: { opacity: 1, scale: [1, 1.025, 1], transition: { duration: 2.6, repeat: Infinity } },
  exit: { opacity: 0, scale: 0.9, transition: { duration: 0.2 } },
};

export const ringVariants: Variants = {
  HIDDEN: { opacity: 0 },
  IDLE: { rotate: 0, opacity: 0.32 },
  LISTENING: { rotate: 0, opacity: 0.62 },
  TRANSCRIBING: { rotate: 360, opacity: 0.52, transition: { duration: 2.4, repeat: Infinity, ease: 'linear' } },
  THINKING: { rotate: 360, opacity: [0.38, 0.72, 0.38], transition: { duration: 3.2, repeat: Infinity, ease: 'linear' } },
  EXECUTING: { rotate: 360, opacity: 0.76, transition: { duration: 1.15, repeat: Infinity, ease: 'linear' } },
  SPEAKING: { rotate: 0, opacity: [0.45, 0.8, 0.45], transition: { duration: 1.1, repeat: Infinity } },
  SUCCESS: { rotate: 0, opacity: [0.5, 1, 0.35], scale: [1, 1.16, 1], transition: { duration: 0.55 } },
  ERROR: { rotate: 0, opacity: 0.62 },
  WAITING_CONFIRMATION: { rotate: 0, opacity: [0.35, 0.68, 0.35], transition: { duration: 2.6, repeat: Infinity } },
};

export const textVariants: Variants = {
  hidden: { opacity: 0, y: 5 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.2 } },
  exit: { opacity: 0, y: -4, transition: { duration: 0.14 } },
};
