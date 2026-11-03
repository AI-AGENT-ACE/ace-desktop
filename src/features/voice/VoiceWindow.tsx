import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, emitTo } from '@tauri-apps/api/event';
import { AnimatePresence, useMotionValue } from 'motion/react';
import { VoiceOverlay } from './VoiceOverlay';
import type { OrbVisualState } from './orbMotion';
import { apiBaseUrl, apiErrorMessage } from '../../api/client';
import { requestVoiceAccessToken } from '../../api/voice-session';
import { RecordingEndpoint } from './recording-endpoint';
import {
  playWakeChime,
  resolveVoiceChoice,
  RETRY_LISTEN_MS,
  SUCCESS_CLOSE_MS,
  type VoiceChoice,
} from './voice-session-policy';
import '../../styles/globals.css';

const localPortfolioMode = import.meta.env.VITE_VOICE_MODE === 'local-portfolio';
const recognitionRetry = '제대로 인식하지 못했습니다. 다시 말씀해주세요.';
type CaptureMode = 'command' | 'retry' | 'confirmation';
type RecordingResult = { recordingId: string };
type ToolResult = {
  requestId: string;
  status: 'EXECUTING' | 'SUCCESS' | 'ERROR' | 'WAITING_CONFIRMATION';
  message: string;
  choices: VoiceChoice[];
};
type UploadResult = {
  result: {
    transcript?: string;
    content?: string;
    type?: string;
    tool?: string;
    arguments?: Record<string, unknown>;
  };
};
const voiceErrorMessage = (cause: unknown) => {
  const raw = typeof cause === 'string' ? cause : cause instanceof Error ? cause.message : '';
  const errors: Record<string, string> = {
    VOICE_RECORDING_EMPTY: recognitionRetry,
    VOICE_RECORDING_TOO_SHORT: recognitionRetry,
    UNAUTHENTICATED: '메인 창에서 다시 로그인해 주세요.',
    AI_SERVER_UNAVAILABLE: '음성 AI 서버가 연결되지 않았어요.',
    VOICE_TIMEOUT: '음성 AI 응답 시간이 초과됐어요. 잠시 후 다시 요청해 주세요.',
    VOICE_INVALID_RESPONSE: '음성 AI의 답변을 처리하지 못했어요. 다시 요청해 주세요.',
    VOICE_PROCESSING_FAILED: '음성 AI 처리에 실패했어요. 다시 요청해 주세요.',
    LOCAL_STT_NOT_INSTALLED: '로컬 음성 인식 모델을 설치해 주세요.',
    LOCAL_STT_FAILED: recognitionRetry,
  };
  return errors[raw] || apiErrorMessage(cause);
};

export function VoiceWindow() {
  const [text, setText] = useState('');
  const [visualState, setVisualState] = useState<OrbVisualState>('IDLE');
  useEffect(() => {
    void invoke('report_voice_phase', { phase: visualState }).catch(() => undefined);
  }, [visualState]);
  const [recording, setRecording] = useState(false);
  const [visible, setVisible] = useState(false);
  const [activation, setActivation] = useState(0);
  const [choices, setChoices] = useState<VoiceChoice[]>([]);
  const [isAnswer, setIsAnswer] = useState(false);
  const [canStartRecording, setCanStartRecording] = useState(false);
  const audioLevel = useMotionValue(0);
  const open = useRef(false);
  const generation = useRef(0);
  const audio = useRef<(() => void)[] | null>(null);
  const writeQueue = useRef<Promise<void>>(Promise.resolve());
  const writeFailure = useRef<unknown>(null);
  const recordingActive = useRef(false);
  const finalizing = useRef(false);
  const finishRef = useRef<() => Promise<void>>(async () => {});
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingId = useRef<string | null>(null);
  const currentChoices = useRef<VoiceChoice[]>([]);
  const unrecognizedChoices = useRef(0);
  const retryMode = useRef<CaptureMode | null>(null);
  const mounted = useRef(true);
  const persistent = useRef(false);
  const restartLock = useRef(false);

  const valid = (token: number) => mounted.current && open.current && generation.current === token;
  const clearTimer = () => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  };
  const updateChoices = (next: VoiceChoice[]) => {
    unrecognizedChoices.current = 0;
    currentChoices.current = next;
    setChoices(next);
  };
  const releaseAudio = async () => {
    const current = audio.current;
    audio.current = null;
    recordingActive.current = false;
    setRecording(false);
    audioLevel.set(0);
    if (!current) return;
    current.forEach((unlisten) => unlisten());
    await invoke('stop_audio_input').catch(() => undefined);
  };
  const close = async () => {
    void emitTo('main', 'ace-speech-stop').catch(() => undefined);
    open.current = false;
    generation.current += 1;
    clearTimer();
    retryMode.current = null;
    pendingId.current = null;
    persistent.current = false;
    setIsAnswer(false);
    setCanStartRecording(false);
    updateChoices([]);
    setVisible(false);
    await releaseAudio();
    await writeQueue.current.catch(() => undefined);
    await invoke('hide_voice_overlay').catch(() => undefined);
  };
  const showSuccess = (message: string, answer = false) => {
    void emitTo('main', 'ace-speech-request', message).catch(() => undefined);
    clearTimer();
    retryMode.current = null;
    updateChoices([]);
    setText(message);
    setVisualState('SUCCESS');
    setIsAnswer(answer);
    if (answer) persistent.current = true;
    setCanStartRecording(persistent.current);
    if (persistent.current) return;
    const token = generation.current;
    timer.current = setTimeout(() => {
      if (valid(token)) void close();
    }, SUCCESS_CLOSE_MS);
  };
  const idleOrClose = async () => {
    if (!persistent.current) return close();
    clearTimer();
    retryMode.current = null;
    setIsAnswer(false);
    setVisualState('IDLE');
    setText('다시 말하려면 Orb를 눌러 주세요.');
    setCanStartRecording(true);
  };
  const startRetryIfReady = () => {
    const next = retryMode.current;
    if (!next || finalizing.current || !open.current) return;
    retryMode.current = null;
    void startCapture(next);
  };
  const retry = (message: string, mode: CaptureMode = 'retry') => {
    clearTimer();
    setText(message);
    setIsAnswer(false);
    setCanStartRecording(false);
    setVisualState(mode === 'confirmation' ? 'WAITING_CONFIRMATION' : 'LISTENING');
    retryMode.current = mode;
    startRetryIfReady();
  };
  const choose = async (choiceId: string) => {
    const id = pendingId.current;
    if (!id || !currentChoices.current.length) return;
    generation.current += 1;
    retryMode.current = null;
    updateChoices([]);
    const token = generation.current;
    await releaseAudio();
    await writeQueue.current.catch(() => undefined);
    if (!valid(token) || pendingId.current !== id) return;
    await invoke('cancel_voice_recording').catch(() => undefined);
    if (!valid(token) || pendingId.current !== id) return;
    setText('선택을 전달하고 있어요.');
    setVisualState('EXECUTING');
    try {
      await invoke('respond_voice_choice', { requestId: id, choiceId });
    } catch (cause) {
      if (open.current) {
        pendingId.current = null;
        await invoke('cancel_voice_request').catch(() => undefined);
        retry(voiceErrorMessage(cause));
      }
    }
  };
  const submit = async (tool: string, args: Record<string, unknown>) => {
    const requestId = crypto.randomUUID();
    pendingId.current = requestId;
    setVisualState('EXECUTING');
    setText('요청을 전달하고 있어요.');
    clearTimer();
    const token = generation.current;
    timer.current = setTimeout(() => {
      if (!valid(token) || pendingId.current !== requestId) return;
      pendingId.current = null;
      void invoke('cancel_voice_request').catch(() => undefined);
      setText('실행 응답을 확인하지 못했어요. 메인 창에서 결과를 확인해 주세요.');
      setVisualState('ERROR');
      timer.current = setTimeout(() => {
        if (valid(token)) void idleOrClose();
      }, 5000);
    }, 10000);
    try {
      await invoke('submit_voice_tool_call', { tool, arguments: args, requestId });
    } catch (cause) {
      clearTimer();
      pendingId.current = null;
      throw cause;
    }
  };
  const startCapture = async (mode: CaptureMode) => {
    await emitTo('main', 'ace-speech-stop').catch(() => undefined);
    const token = ++generation.current;
    clearTimer();
    await releaseAudio();
    if (!valid(token)) return;
    setIsAnswer(false);
    setCanStartRecording(false);
    setVisualState(mode === 'confirmation' ? 'WAITING_CONFIRMATION' : 'LISTENING');
    if (mode === 'command') setText('마이크를 준비하고 있어요.');
    try {
      const subscriptions: (() => void)[] = [];
      audio.current = subscriptions;
      recordingActive.current = true;
      setRecording(true);
      writeFailure.current = null;
      writeQueue.current = Promise.resolve();
      const endpoint = new RecordingEndpoint(RETRY_LISTEN_MS, mode === 'confirmation' ? 120 : 250);
      let stopping = false;
      const finish = async () => {
        if (stopping || !valid(token) || !recordingActive.current) return;
        stopping = true;
        finalizing.current = true;
        try {
          await releaseAudio();
          await writeQueue.current;
          if (!valid(token)) return;
          if (writeFailure.current) {
            await invoke('cancel_voice_recording');
            throw new Error('음성을 저장하지 못했어요.');
          }
          if (!endpoint.hasSpeech) {
            await invoke('cancel_voice_recording');
            if (mode === 'confirmation') {
              await choose(
                currentChoices.current.some((choice) => choice.id === 'no') ? 'no' : 'cancel',
              );
            } else if (mode === 'command') retry(recognitionRetry);
            else await idleOrClose();
            return;
          }
          setVisualState('TRANSCRIBING');
          setText(mode === 'confirmation' ? '선택을 확인하고 있어요.' : '말씀을 확인하고 있어요.');
          const result = await invoke<RecordingResult>('stop_voice_recording');
          if (!valid(token)) {
            await invoke('discard_voice_recording', { recordingId: result.recordingId }).catch(
              () => undefined,
            );
            return;
          }
          if (localPortfolioMode || mode === 'confirmation') {
            const recognized = await invoke<{ transcript: string; portfolioRequested: boolean }>(
              'transcribe_local_portfolio',
              { recordingId: result.recordingId },
            );
            if (!valid(token)) return;
            if (mode === 'confirmation') {
              const choice = resolveVoiceChoice(recognized.transcript, currentChoices.current);
              if (import.meta.env.DEV)
                console.info('VOICE_CHOICE_RESOLVED', {
                  requestId: pendingId.current,
                  recognized: recognized.transcript.trim().length > 0,
                  outcome:
                    choice === 'no' || choice === 'cancel'
                      ? 'cancel'
                      : choice
                        ? 'selected'
                        : 'unmatched',
                });
              if (choice) await choose(choice);
              else if (++unrecognizedChoices.current >= 3)
                await choose(
                  currentChoices.current.some((choice) => choice.id === 'no') ? 'no' : 'cancel',
                );
              else
                retry(
                  '선택을 확인하지 못했어요. 네, 아니오 또는 선택지 번호를 말해 주세요.',
                  'confirmation',
                );
            } else if (recognized.portfolioRequested) {
              await submit('file.open', { directory: 'desktop', path: '김환성_포트폴리오.pdf' });
            } else retry(recognitionRetry);
          } else {
            const accessToken = await requestVoiceAccessToken().catch(async (cause) => {
              await invoke('discard_voice_recording', { recordingId: result.recordingId });
              throw cause;
            });
            if (!valid(token)) {
              await invoke('discard_voice_recording', { recordingId: result.recordingId }).catch(
                () => undefined,
              );
              return;
            }
            setVisualState('THINKING');
            const uploaded = await invoke<UploadResult>('upload_voice_recording', {
              recordingId: result.recordingId,
              apiBaseUrl,
              accessToken,
              conversationId: null,
            });
            if (!valid(token)) return;
            if (!uploaded.result.transcript?.trim()) {
              retry(recognitionRetry);
              return;
            }
            if (
              uploaded.result.type === 'tool_call' &&
              uploaded.result.tool &&
              uploaded.result.arguments
            )
              await submit(uploaded.result.tool, uploaded.result.arguments);
            else
              showSuccess(
                uploaded.result.content || uploaded.result.transcript || '완료했어요.',
                true,
              );
          }
        } catch (cause) {
          if (valid(token)) {
            if (mode === 'confirmation' && ++unrecognizedChoices.current >= 3)
              await choose(
                currentChoices.current.some((choice) => choice.id === 'no') ? 'no' : 'cancel',
              );
            else
              retry(voiceErrorMessage(cause), mode === 'confirmation' ? 'confirmation' : 'retry');
          }
        } finally {
          finalizing.current = false;
          startRetryIfReady();
        }
      };
      finishRef.current = finish;
      const sessionId = crypto.randomUUID();
      subscriptions.push(
        await listen<{ sessionId: string; sampleRate: number; rms: number; samples: number[] }>(
          'ace-audio-input',
          ({ payload }) => {
            if (
              payload.sessionId !== sessionId ||
              !valid(token) ||
              !recordingActive.current ||
              stopping
            )
              return;
            audioLevel.set(Math.min(1, payload.rms / 0.15));
            const samples = payload.samples;
            writeQueue.current = writeQueue.current
              .then(() => invoke<void>('append_voice_recording_samples', { samples }))
              .catch((cause) => {
                writeFailure.current = cause;
              });
            if (endpoint.feed(payload.rms, (samples.length / payload.sampleRate) * 1000))
              void finish();
          },
        ),
      );
      subscriptions.push(
        await listen<{ sessionId: string; message: string }>(
          'ace-audio-input-error',
          ({ payload }) => {
            if (payload.sessionId !== sessionId || !valid(token)) return;
            generation.current += 1;
            clearTimer();
            retryMode.current = null;
            setCanStartRecording(false);
            void releaseAudio()
              .then(() => writeQueue.current)
              .then(() => invoke('cancel_voice_recording'))
              .catch(() => undefined)
              .finally(() => {
                if (open.current && mounted.current) setCanStartRecording(true);
              });
            setVisualState('ERROR');
            setText(payload.message);
          },
        ),
      );
      subscriptions.push(
        await listen('ace-audio-input-changed', () => {
          if (!valid(token)) return;
          generation.current += 1;
          clearTimer();
          retryMode.current = null;
          setCanStartRecording(false);
          void releaseAudio()
            .then(() => writeQueue.current)
            .then(() => invoke('cancel_voice_recording'))
            .catch(() => undefined)
            .finally(() => {
              if (!open.current || !mounted.current) return;
              if (currentChoices.current.length) {
                retryMode.current = 'confirmation';
                startRetryIfReady();
              } else setCanStartRecording(true);
            });
          setVisualState(currentChoices.current.length ? 'WAITING_CONFIRMATION' : 'IDLE');
          setText(
            currentChoices.current.length
              ? '마이크를 변경했습니다. 선택을 말씀해 주세요.'
              : '마이크를 변경했습니다. Orb를 눌러 다시 말씀해 주세요.',
          );
        }),
      );
      if (!valid(token)) {
        subscriptions.forEach((fn) => fn());
        return;
      }
      // Pause delivery until the WAV writer is ready; discard early PCM chunks.
      recordingActive.current = false;
      const sampleRate = await invoke<number>('start_audio_input', { sessionId, monitor: false });
      if (!valid(token)) {
        await releaseAudio();
        return;
      }
      await invoke('start_voice_recording', { sampleRate, channels: 1 });
      if (!valid(token)) {
        await releaseAudio();
        await invoke('cancel_voice_recording');
        return;
      }
      recordingActive.current = true;
      if (mode === 'command') {
        setText('듣고 있어요. 말씀해 주세요.');
        // Signal readiness only after the microphone and PCM callback are connected.
        void playWakeChime().catch(() => undefined);
      }
    } catch (cause) {
      if (!valid(token)) return;
      await releaseAudio();
      await invoke('cancel_voice_recording').catch(() => undefined);
      setVisualState('ERROR');
      setText(`마이크를 시작하지 못했어요. ${voiceErrorMessage(cause)}`);
      setCanStartRecording(persistent.current && mode !== 'confirmation');
      if (mode !== 'confirmation' && !persistent.current)
        timer.current = setTimeout(() => {
          if (valid(token)) void close();
        }, 5000);
    }
  };

  useEffect(() => {
    mounted.current = true;
    document.documentElement.dataset.theme = localStorage.getItem('ace-theme') || 'light';
    const unlisteners: (() => void)[] = [];
    let disposed = false;
    const register = async <T,>(name: string, handler: (payload: T) => void) => {
      const unlisten = await listen<T>(name, (event) => handler(event.payload));
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    };
    void register('ace-voice-activate', () => {
      open.current = true;
      clearTimer();
      setVisible(true);
      setActivation((value) => value + 1);
      updateChoices([]);
      setText('듣기 준비 중이에요.');
      setVisualState('LISTENING');
      void startCapture('command');
    });
    void register('ace-voice-reset', () => {
      open.current = false;
      generation.current += 1;
      clearTimer();
      retryMode.current = null;
      pendingId.current = null;
      persistent.current = false;
      setIsAnswer(false);
      setCanStartRecording(false);
      updateChoices([]);
      setVisible(false);
      void releaseAudio();
    });
    void register<ToolResult>('ace-tool-confirmation-open', async (result) => {
      persistent.current = false;
      setIsAnswer(false);
      setCanStartRecording(false);
      const token = ++generation.current;
      clearTimer();
      retryMode.current = null;
      await releaseAudio();
      await writeQueue.current.catch(() => undefined);
      await invoke('cancel_voice_recording').catch(() => undefined);
      if (generation.current !== token) return;
      open.current = true;
      setVisible(true);
      pendingId.current = result.requestId;
      setText(result.message);
      updateChoices(result.choices);
      setVisualState('WAITING_CONFIRMATION');
      retryMode.current = 'confirmation';
      startRetryIfReady();
    });
    void register<ToolResult>('ace-voice-tool-result', async (result) => {
      if (!open.current || result.requestId !== pendingId.current) return;
      clearTimer();
      setText(result.message);
      // A confirmation can also be answered in the main window.
      if (
        result.status !== 'WAITING_CONFIRMATION' &&
        (recordingActive.current || currentChoices.current.length)
      ) {
        generation.current += 1;
        retryMode.current = null;
        updateChoices([]);
        await releaseAudio();
        await writeQueue.current.catch(() => undefined);
        await invoke('cancel_voice_recording').catch(() => undefined);
        if (!open.current || result.requestId !== pendingId.current) return;
      }
      if (result.status === 'WAITING_CONFIRMATION') {
        updateChoices(result.choices);
        retryMode.current = 'confirmation';
        setVisualState('WAITING_CONFIRMATION');
        startRetryIfReady();
      } else if (result.status === 'EXECUTING') setVisualState('EXECUTING');
      else {
        pendingId.current = null;
        updateChoices([]);
        if (result.status === 'SUCCESS') showSuccess(result.message);
        else retry(`${result.message} 5초 안에 다시 말씀해 주세요.`);
      }
    });
    return () => {
      disposed = true;
      mounted.current = false;
      open.current = false;
      generation.current += 1;
      clearTimer();
      unlisteners.forEach((stop) => stop());
      void releaseAudio();
      void invoke('cancel_voice_recording').catch(() => undefined);
    };
  }, []);

  return (
    <div className="voice-window">
      <AnimatePresence>
        {visible && (
          <VoiceOverlay
            key={activation}
            visualState={visualState}
            transcript={text}
            audioLevel={audioLevel}
            recording={recording}
            isAnswer={isAnswer}
            canStartRecording={canStartRecording}
            onStartRecording={() => {
              if (
                restartLock.current ||
                recordingActive.current ||
                finalizing.current ||
                pendingId.current
              )
                return;
              restartLock.current = true;
              void startCapture('command').finally(() => {
                restartLock.current = false;
              });
            }}
            choices={choices}
            onChoice={(choice) => void choose(choice)}
            onClose={() => void close()}
            onStopRecording={() => void finishRef.current()}
          />
        )}
      </AnimatePresence>
    </div>
  );
}
