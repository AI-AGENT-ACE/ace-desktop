import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
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
    VOICE_RECORDING_EMPTY: '음성을 듣지 못했어요. 다시 말씀해 주세요.',
    VOICE_RECORDING_TOO_SHORT: '말씀을 끝까지 듣지 못했어요. 다시 말씀해 주세요.',
    UNAUTHENTICATED: '메인 창에서 다시 로그인해 주세요.',
    AI_SERVER_UNAVAILABLE: '음성 AI 서버가 연결되지 않았어요.',
    LOCAL_STT_NOT_INSTALLED: '로컬 음성 인식 모델을 설치해 주세요.',
    LOCAL_STT_FAILED: '음성을 인식하지 못했어요. 다시 말씀해 주세요.',
  };
  return errors[raw] || apiErrorMessage(cause);
};

export function VoiceWindow() {
  const [text, setText] = useState('');
  const [visualState, setVisualState] = useState<OrbVisualState>('IDLE');
  const [recording, setRecording] = useState(false);
  const [visible, setVisible] = useState(false);
  const [activation, setActivation] = useState(0);
  const [choices, setChoices] = useState<VoiceChoice[]>([]);
  const audioLevel = useMotionValue(0);
  const open = useRef(false);
  const generation = useRef(0);
  const audio = useRef<{
    stream: MediaStream;
    context: AudioContext;
    source: MediaStreamAudioSourceNode;
    processor: ScriptProcessorNode;
    gain: GainNode;
  } | null>(null);
  const writeQueue = useRef<Promise<void>>(Promise.resolve());
  const writeFailure = useRef<unknown>(null);
  const recordingActive = useRef(false);
  const finalizing = useRef(false);
  const finishRef = useRef<() => Promise<void>>(async () => {});
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingId = useRef<string | null>(null);
  const currentChoices = useRef<VoiceChoice[]>([]);
  const retryMode = useRef<CaptureMode | null>(null);
  const mounted = useRef(true);

  const valid = (token: number) => mounted.current && open.current && generation.current === token;
  const clearTimer = () => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  };
  const updateChoices = (next: VoiceChoice[]) => {
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
    current.processor.onaudioprocess = null;
    current.processor.disconnect();
    current.source.disconnect();
    current.gain.disconnect();
    current.stream.getTracks().forEach((track) => track.stop());
    await current.context.close().catch(() => undefined);
  };
  const close = async () => {
    open.current = false;
    generation.current += 1;
    clearTimer();
    retryMode.current = null;
    pendingId.current = null;
    updateChoices([]);
    setVisible(false);
    await releaseAudio();
    await writeQueue.current.catch(() => undefined);
    await invoke('hide_voice_overlay').catch(() => undefined);
  };
  const showSuccess = (message: string) => {
    clearTimer();
    retryMode.current = null;
    updateChoices([]);
    setText(message);
    setVisualState('SUCCESS');
    const token = generation.current;
    timer.current = setTimeout(() => {
      if (valid(token)) void close();
    }, SUCCESS_CLOSE_MS);
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
        if (valid(token)) void close();
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
    const token = ++generation.current;
    clearTimer();
    await releaseAudio();
    if (!valid(token)) return;
    setVisualState(mode === 'confirmation' ? 'WAITING_CONFIRMATION' : 'LISTENING');
    if (mode === 'command') setText('듣고 있어요. 말씀해 주세요.');
    let stream: MediaStream | null = null;
    let context: AudioContext | null = null;
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });
      if (!valid(token)) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }
      context = new AudioContext();
      await context.resume();
      if (!valid(token)) {
        stream.getTracks().forEach((track) => track.stop());
        await context.close().catch(() => undefined);
        return;
      }
      const source = context.createMediaStreamSource(stream);
      const processor = context.createScriptProcessor(4096, 1, 1);
      const gain = context.createGain();
      gain.gain.value = 0;
      audio.current = { stream, context, source, processor, gain };
      await invoke('start_voice_recording', {
        sampleRate: Math.round(context.sampleRate),
        channels: 1,
      });
      if (!valid(token)) return;
      recordingActive.current = true;
      setRecording(true);
      writeFailure.current = null;
      writeQueue.current = Promise.resolve();
      const endpoint = new RecordingEndpoint(
        mode === 'command' ? 10000 : RETRY_LISTEN_MS,
        mode === 'confirmation' ? 120 : 250,
      );
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
            if (mode === 'confirmation') retryMode.current = 'confirmation';
            else await close();
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
              if (choice) await choose(choice);
              else
                retry(
                  '선택을 확인하지 못했어요. 네, 아니오 또는 선택지 번호를 말해 주세요.',
                  'confirmation',
                );
            } else if (recognized.portfolioRequested) {
              await submit('file.open', { directory: 'desktop', path: '김환성_포트폴리오.pdf' });
            } else
              retry(
                recognized.transcript
                  ? `“${recognized.transcript}”로 들었어요. 포트폴리오를 열려면 다시 말씀해 주세요. (5초 안에 시작)`
                  : '음성을 인식하지 못했어요. 5초 안에 다시 말씀해 주세요.',
              );
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
            if (
              uploaded.result.type === 'tool_call' &&
              uploaded.result.tool &&
              uploaded.result.arguments
            )
              await submit(uploaded.result.tool, uploaded.result.arguments);
            else
              showSuccess(uploaded.result.content || uploaded.result.transcript || '완료했어요.');
          }
        } catch (cause) {
          if (valid(token))
            retry(
              `${voiceErrorMessage(cause)} 5초 안에 다시 말씀해 주세요.`,
              mode === 'confirmation' ? 'confirmation' : 'retry',
            );
        } finally {
          finalizing.current = false;
          startRetryIfReady();
        }
      };
      finishRef.current = finish;
      processor.onaudioprocess = (event) => {
        if (!valid(token) || !recordingActive.current || stopping) return;
        const channel = event.inputBuffer.getChannelData(0);
        const rms = Math.sqrt(
          channel.reduce((sum, sample) => sum + sample * sample, 0) / channel.length,
        );
        audioLevel.set(Math.min(1, rms / 0.15));
        const samples = Array.from(channel);
        writeQueue.current = writeQueue.current
          .then(() => invoke<void>('append_voice_recording_samples', { samples }))
          .catch((cause) => {
            writeFailure.current = cause;
          });
        if (endpoint.feed(rms, (channel.length / event.inputBuffer.sampleRate) * 1000))
          void finish();
      };
      source.connect(processor);
      processor.connect(gain);
      gain.connect(context.destination);
    } catch (cause) {
      stream?.getTracks().forEach((track) => track.stop());
      if (context && context.state !== 'closed') await context.close().catch(() => undefined);
      if (!valid(token)) return;
      await releaseAudio();
      await invoke('cancel_voice_recording').catch(() => undefined);
      setVisualState('ERROR');
      setText(`마이크를 시작하지 못했어요. ${voiceErrorMessage(cause)}`);
      if (mode !== 'confirmation')
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
      const token = ++generation.current;
      void playWakeChime()
        .catch(() => undefined)
        .then(() => {
          if (valid(token)) void startCapture('command');
        });
    });
    void register('ace-voice-reset', () => {
      open.current = false;
      generation.current += 1;
      clearTimer();
      retryMode.current = null;
      pendingId.current = null;
      updateChoices([]);
      setVisible(false);
      void releaseAudio();
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
