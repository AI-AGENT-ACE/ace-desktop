import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { AnimatePresence, useMotionValue } from 'motion/react';
import { VoiceOverlay } from './VoiceOverlay';
import { apiBaseUrl, apiErrorMessage } from '../../api/client';
import { requestVoiceAccessToken } from '../../api/voice-session';
import { RecordingEndpoint } from './recording-endpoint';
import '../../styles/globals.css';
const localPortfolioMode = import.meta.env.VITE_VOICE_MODE === 'local-portfolio';

type RecordingResult = {
  recordingId: string;
  state: 'READY';
  sampleRate: number;
  channels: number;
  samplesWritten: number;
  durationMs: number;
};
type VoicePhase =
  | 'IDLE'
  | 'LISTENING'
  | 'RECORDING'
  | 'FINALIZING'
  | 'READY'
  | 'UPLOADING'
  | 'PROCESSING'
  | 'EXECUTING'
  | 'SUCCESS'
  | 'ERROR';
type UploadResult = {
  recordingId: string;
  state: 'SUCCESS';
  result: {
    transcript?: string;
    content?: string;
    type?: string;
    tool?: string;
    arguments?: Record<string, unknown>;
  };
};
const voiceErrors: Record<string, string> = {
  RECORDING_FAILED: '음성 녹음 중 문제가 발생했습니다.',
  WAV_FINALIZE_FAILED: '음성 녹음 중 문제가 발생했습니다.',
  VOICE_RECORDING_EMPTY: '음성을 인식하지 못했습니다.',
  VOICE_RECORDING_TOO_SHORT: '음성을 인식하지 못했습니다.',
  VOICE_UPLOAD_FAILED: '음성을 인식하는 중 문제가 발생했습니다.',
  VOICE_PROCESSING_FAILED: '음성을 인식하는 중 문제가 발생했습니다.',
  VOICE_TIMEOUT: '음성을 인식하는 중 문제가 발생했습니다.',
  VOICE_FILE_INVALID: '음성을 인식하지 못했습니다.',
  DUPLICATE_VOICE_SESSION: '이미 음성 요청을 처리하고 있습니다.',
  UNAUTHENTICATED: '로그인이 만료되었습니다. 메인 창에서 다시 로그인해 주세요.',
  AI_SERVER_UNAVAILABLE:
    '음성 AI 서버가 연결되지 않았습니다. 백엔드의 AI_SERVER_URL 설정을 확인해 주세요.',
  LOCAL_STT_NOT_INSTALLED: '로컬 음성 인식 모델을 설치해 주세요.',
  LOCAL_STT_FAILED: '로컬 음성 인식에 실패했습니다. 다시 말해 주세요.',
};
const voiceErrorMessage = (cause: unknown) => {
  const raw = typeof cause === 'string' ? cause : cause instanceof Error ? cause.message : '';
  return voiceErrors[raw] || apiErrorMessage(cause);
};

export function VoiceWindow() {
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const [recording, setRecording] = useState(false);
  const [phase, setPhase] = useState<VoicePhase>('IDLE');
  const [visible, setVisible] = useState(true);
  const audioLevel = useMotionValue(0);
  const smoothedLevel = useRef(0);
  const microphone = useRef<MediaStream | null>(null);
  const audioContext = useRef<AudioContext | null>(null);
  const audioSource = useRef<MediaStreamAudioSourceNode | null>(null);
  const audioProcessor = useRef<ScriptProcessorNode | null>(null);
  const silentGain = useRef<GainNode | null>(null);
  const writeQueue = useRef<Promise<void>>(Promise.resolve());
  const recordingActive = useRef(false);
  const finalizing = useRef(false);
  const writeFailure = useRef<unknown>(null);
  const microphoneGeneration = useRef(0);
  const speechDetected = useRef(false);

  const releaseAudio = async () => {
    microphoneGeneration.current += 1;
    if (audioProcessor.current) {
      audioProcessor.current.onaudioprocess = null;
      audioProcessor.current.disconnect();
    }
    audioSource.current?.disconnect();
    silentGain.current?.disconnect();
    const context = audioContext.current;
    microphone.current?.getTracks().forEach((track) => track.stop());
    audioProcessor.current = null;
    audioSource.current = null;
    silentGain.current = null;
    audioContext.current = null;
    microphone.current = null;
    smoothedLevel.current = 0;
    audioLevel.set(0);
    if (context) await context.close().catch(() => undefined);
  };

  const voiceLog = (message: string, detail?: unknown) => {
    if (import.meta.env.DEV) console.debug(`[Voice] ${message}`, detail ?? '');
  };

  const stopAndSave = async () => {
    if (!recordingActive.current || finalizing.current) return null;
    finalizing.current = true;
    recordingActive.current = false;
    setRecording(false);
    setPhase('FINALIZING');
    voiceLog('stop requested');
    try {
      await releaseAudio();
      voiceLog('recording finalizing');
      await writeQueue.current;
      if (writeFailure.current) {
        voiceLog('PCM write failed', writeFailure.current);
        await invoke('cancel_voice_recording').catch(() => undefined);
        throw new Error('RECORDING_FAILED');
      }
      const result = await invoke<RecordingResult>('stop_voice_recording');
      voiceLog('recording finalized', {
        samples: result.samplesWritten,
        durationMs: result.durationMs,
      });
      setPhase('READY');
      if (localPortfolioMode) {
        if (!speechDetected.current) {
          await invoke('discard_voice_recording', { recordingId: result.recordingId });
          throw new Error('VOICE_RECORDING_EMPTY');
        }
        setPhase('PROCESSING');
        const generation = microphoneGeneration.current;
        const recognized = await invoke<{ transcript: string; portfolioRequested: boolean }>(
          'transcribe_local_portfolio',
          { recordingId: result.recordingId },
        );
        if (generation !== microphoneGeneration.current) return result;
        setText(recognized.transcript);
        if (!recognized.portfolioRequested) {
          throw new Error(
            recognized.transcript
              ? `“${recognized.transcript}”로 들었어요. “바탕화면에서 포트폴리오 열어줘”라고 말해 주세요.`
              : '음성을 인식하지 못했습니다. 다시 말해 주세요.',
          );
        }
        setPhase('EXECUTING');
        await invoke('submit_voice_tool_call', {
          tool: 'file.open',
          arguments: { directory: 'desktop', path: '김환성_포트폴리오.pdf' },
        });
        setVisible(false);
        setPhase('IDLE');
        return result;
      }
      const accessToken = await requestVoiceAccessToken().catch(async (cause) => {
        await invoke('discard_voice_recording', { recordingId: result.recordingId });
        throw cause;
      });
      setPhase('UPLOADING');
      voiceLog('transcription started');
      const processingTimer = window.setTimeout(() => setPhase('PROCESSING'), 500);
      const uploaded = await invoke<UploadResult>('upload_voice_recording', {
        recordingId: result.recordingId,
        apiBaseUrl,
        accessToken,
        conversationId: null,
      }).finally(() => window.clearTimeout(processingTimer));
      if (uploaded.result.transcript || uploaded.result.content)
        setText(uploaded.result.transcript || uploaded.result.content || '');
      if (
        uploaded.result.type === 'tool_call' &&
        uploaded.result.tool &&
        uploaded.result.arguments
      ) {
        setPhase('EXECUTING');
        await new Promise((resolve) => window.setTimeout(resolve, 420));
        setPhase('SUCCESS');
        await new Promise((resolve) => window.setTimeout(resolve, 450));
        setVisible(false);
        await new Promise((resolve) => window.setTimeout(resolve, 200));
        await invoke('submit_voice_tool_call', {
          tool: uploaded.result.tool,
          arguments: uploaded.result.arguments,
        });
      } else {
        setPhase('SUCCESS');
        await new Promise((resolve) => window.setTimeout(resolve, 650));
        setVisible(false);
        await new Promise((resolve) => window.setTimeout(resolve, 200));
        await invoke('hide_voice_overlay');
      }
      return result;
    } finally {
      finalizing.current = false;
    }
  };

  const cancelTemporary = async () => {
    await releaseAudio();
    await writeQueue.current;
    if (!recordingActive.current) return;
    recordingActive.current = false;
    setRecording(false);
    setPhase('IDLE');
    await invoke('cancel_voice_recording');
  };

  const startMicrophone = async () => {
    await releaseAudio();
    const generation = microphoneGeneration.current;
    setRecording(false);
    setPhase('LISTENING');
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
      });
      if (generation !== microphoneGeneration.current) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }

      const context = new AudioContext();
      const source = context.createMediaStreamSource(stream);
      const processor = context.createScriptProcessor(4096, 1, 1);
      const gain = context.createGain();
      gain.gain.value = 0;
      await invoke('start_voice_recording', {
        sampleRate: Math.round(context.sampleRate),
        channels: 1,
      });

      microphone.current = stream;
      audioContext.current = context;
      audioSource.current = source;
      audioProcessor.current = processor;
      silentGain.current = gain;
      recordingActive.current = true;
      writeFailure.current = null;
      writeQueue.current = Promise.resolve();
      setRecording(true);
      setPhase('RECORDING');
      const endpoint = new RecordingEndpoint();
      speechDetected.current = false;
      processor.onaudioprocess = (event) => {
        if (!recordingActive.current || finalizing.current) return;
        const channel = event.inputBuffer.getChannelData(0);
        let squareSum = 0;
        for (let index = 0; index < channel.length; index += 1)
          squareSum += channel[index] * channel[index];
        const rms = Math.sqrt(squareSum / channel.length);
        const normalized = Math.min(1, Math.max(0, (rms - 0.035) / 0.28));
        smoothedLevel.current = smoothedLevel.current * 0.8 + normalized * 0.2;
        audioLevel.set(smoothedLevel.current);
        const samples = Array.from(channel);
        writeQueue.current = writeQueue.current
          .then(() => invoke<void>('append_voice_recording_samples', { samples }))
          .catch((cause) => {
            writeFailure.current = cause;
            voiceLog('PCM append failed', cause);
          });
        const end = endpoint.feed(rms, (channel.length / context.sampleRate) * 1000);
        speechDetected.current = endpoint.hasSpeech;
        if (end) {
          voiceLog('automatic stop requested', { reason: end });
          void stopAndSave().catch((cause) => {
            setPhase('ERROR');
            setError(voiceErrorMessage(cause));
          });
        }
      };
      source.connect(processor);
      processor.connect(gain);
      gain.connect(context.destination);
    } catch (cause) {
      await releaseAudio();
      if (recordingActive.current) {
        recordingActive.current = false;
        void invoke('cancel_voice_recording');
      }
      setRecording(false);
      setPhase('ERROR');
      setError(
        `마이크를 시작하지 못했습니다. Windows와 ACE의 마이크 권한을 확인해 주세요. ${voiceErrorMessage(cause)}`,
      );
    }
  };

  useEffect(() => {
    document.documentElement.dataset.theme = localStorage.getItem('ace-theme') || 'light';
  }, []);

  useEffect(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    const register = async (event: string, callback: () => void) => {
      const unlisten = await listen(event, callback);
      if (disposed) unlisten();
      else stops.push(unlisten);
    };
    void register('ace-voice-activate', () => {
      setVisible(true);
      setText('');
      setError('');
      void startMicrophone();
    });
    void register('ace-voice-reset', () => {
      void releaseAudio();
      recordingActive.current = false;
      setRecording(false);
      setPhase('IDLE');
      setText('');
    });
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
      void releaseAudio();
      if (recordingActive.current) void invoke('cancel_voice_recording');
      recordingActive.current = false;
    };
  }, []);

  const close = async () => {
    setError('');
    try {
      await cancelTemporary();
      setText('');
      setVisible(false);
      await new Promise((resolve) => window.setTimeout(resolve, 200));
      await invoke('hide_voice_overlay');
    } catch (cause) {
      setError(apiErrorMessage(cause));
    } finally {
      setPhase('IDLE');
    }
  };

  const visualState =
    error || phase === 'ERROR'
      ? 'ERROR'
      : phase === 'FINALIZING' || phase === 'UPLOADING'
        ? 'TRANSCRIBING'
        : phase === 'PROCESSING'
          ? 'THINKING'
          : phase === 'EXECUTING'
            ? 'EXECUTING'
            : phase === 'SUCCESS'
              ? 'SUCCESS'
              : phase === 'IDLE'
                ? 'IDLE'
                : 'LISTENING';

  return (
    <div className="voice-window">
      <AnimatePresence>
        {visible && (
          <VoiceOverlay
            visualState={visualState}
            transcript={error || text}
            audioLevel={audioLevel}
            onClose={() => void close()}
            recording={recording}
            onStopRecording={() =>
              void stopAndSave().catch((cause) => {
                voiceLog('manual stop failed', cause);
                setPhase('ERROR');
                setError(voiceErrorMessage(cause));
              })
            }
          />
        )}
      </AnimatePresence>
    </div>
  );
}
