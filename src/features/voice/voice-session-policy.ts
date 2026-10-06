export type VoiceChoice = { id: string; label: string };
export const SUCCESS_CLOSE_MS = 3000;
export const RETRY_LISTEN_MS = 5000;

export function resolveVoiceChoice(transcript: string, choices: VoiceChoice[]): string | null {
  const text = transcript.replace(/[\s.,!?。]/g, '').toLowerCase();
  if (['아니오', '아니요', '아니', '취소', '취소해줘', 'no'].includes(text))
    return choices.find((choice) => choice.id === 'no')?.id ?? (choices.length ? 'cancel' : null);
  if (['네', '예', '응', '좋아요', '실행', '실행해줘', 'yes'].includes(text))
    return choices.find((choice) => choice.id === 'yes')?.id ?? null;
  const aliases = [
    ['1', '1번', '일', '일번', '첫번째'],
    ['2', '2번', '이', '이번', '두번째'],
    ['3', '3번', '삼', '삼번', '세번째'],
  ];
  const index = aliases.findIndex((values) => values.includes(text));
  return index >= 0 ? (choices[index]?.id ?? null) : null;
}

export function wakeChimeEnabled(): boolean {
  return localStorage.getItem('ace-wake-chime') !== 'off';
}

export async function playWakeChime(): Promise<void> {
  if (!wakeChimeEnabled()) return;
  const context = new AudioContext();
  try {
    let timeout: ReturnType<typeof setTimeout> | undefined;
    await Promise.race([
      context.resume(),
      new Promise<void>((resolve) => {
        timeout = setTimeout(resolve, 150);
      }),
    ]).finally(() => clearTimeout(timeout));
    if (context.state !== 'running') return;
    const now = context.currentTime;
    for (const [offset, frequency] of [
      [0, 660],
      [0.09, 880],
    ]) {
      const tone = context.createOscillator();
      const gain = context.createGain();
      tone.type = 'sine';
      tone.frequency.value = frequency;
      gain.gain.setValueAtTime(0, now + offset);
      gain.gain.linearRampToValueAtTime(0.09, now + offset + 0.015);
      gain.gain.exponentialRampToValueAtTime(0.001, now + offset + 0.17);
      tone.connect(gain);
      gain.connect(context.destination);
      tone.start(now + offset);
      tone.stop(now + offset + 0.18);
    }
    await new Promise((resolve) => setTimeout(resolve, 300));
  } finally {
    await context.close().catch(() => undefined);
  }
}
