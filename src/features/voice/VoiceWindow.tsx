import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { VoiceOverlay } from './VoiceOverlay';
import { apiErrorMessage } from '../../api/client';
import '../../styles/globals.css';

type RecordingResult = {
  path: string;
  sampleRate: number;
  channels: number;
  samplesWritten: number;
  durationMs: number;
};

export function VoiceWindow() {
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [recording, setRecording] = useState(false);
  const [savedRecording, setSavedRecording] = useState<RecordingResult | null>(null);
  const microphone = useRef<MediaStream | null>(null);
  const audioContext = useRef<AudioContext | null>(null);
  const audioSource = useRef<MediaStreamAudioSourceNode | null>(null);
  const audioProcessor = useRef<ScriptProcessorNode | null>(null);
  const silentGain = useRef<GainNode | null>(null);
  const writeQueue = useRef<Promise<void>>(Promise.resolve());
  const recordingActive = useRef(false);
  const microphoneGeneration = useRef(0);

  const releaseAudio = () => {
    microphoneGeneration.current += 1;
    if (audioProcessor.current) {
      audioProcessor.current.onaudioprocess = null;
      audioProcessor.current.disconnect();
    }
    audioSource.current?.disconnect();
    silentGain.current?.disconnect();
    if (audioContext.current) void audioContext.current.close();
    microphone.current?.getTracks().forEach((track) => track.stop());
    audioProcessor.current = null;
    audioSource.current = null;
    silentGain.current = null;
    audioContext.current = null;
    microphone.current = null;
  };

  const stopAndSave = async () => {
    releaseAudio();
    await writeQueue.current;
    if (!recordingActive.current) return null;
    recordingActive.current = false;
    setRecording(false);
    const result = await invoke<RecordingResult>('stop_voice_recording');
    setSavedRecording(result);
    return result;
  };

  const startMicrophone = async () => {
    releaseAudio();
    const generation = microphoneGeneration.current;
    setRecording(false);
    setSavedRecording(null);
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
      writeQueue.current = Promise.resolve();
      setRecording(true);
      processor.onaudioprocess = (event) => {
        const samples = Array.from(event.inputBuffer.getChannelData(0));
        writeQueue.current = writeQueue.current
          .then(() => invoke<void>('append_voice_recording_samples', { samples }))
          .catch((cause) => {
            setError(`녹음 데이터를 저장하지 못했습니다. ${apiErrorMessage(cause)}`);
          });
      };
      source.connect(processor);
      processor.connect(gain);
      gain.connect(context.destination);
    } catch (cause) {
      releaseAudio();
      if (recordingActive.current) {
        recordingActive.current = false;
        void invoke('cancel_voice_recording');
      }
      setRecording(false);
      setError(
        `마이크를 시작하지 못했습니다. Windows와 ACE의 마이크 권한을 확인해 주세요. ${apiErrorMessage(cause)}`,
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
      setText('');
      setError('');
      void startMicrophone();
    });
    void register('ace-voice-reset', () => {
      releaseAudio();
      recordingActive.current = false;
      setRecording(false);
      setText('');
    });
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
      releaseAudio();
      if (recordingActive.current) void invoke('cancel_voice_recording');
      recordingActive.current = false;
    };
  }, []);

  const finish = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError('');
    try {
      const transient = text.trim();
      setText('');
      await stopAndSave();
      await invoke('submit_voice_command', { text: transient });
    } catch (cause) {
      setError(apiErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const close = async () => {
    setBusy(true);
    setError('');
    try {
      await stopAndSave();
      setText('');
      await invoke('hide_voice_overlay');
    } catch (cause) {
      setError(apiErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="voice-window">
      <VoiceOverlay
        state={busy ? 'processing' : 'listening'}
        transcript={text}
        onTranscript={setText}
        onFinish={() => void finish()}
        onClose={() => void close()}
        recording={recording}
        recordingPath={savedRecording?.path}
        onStopRecording={() =>
          void stopAndSave().catch((cause) => setError(apiErrorMessage(cause)))
        }
      />
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
