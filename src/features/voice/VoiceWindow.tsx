import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { VoiceOverlay } from './VoiceOverlay';
import { apiErrorMessage } from '../../api/client';
import '../../styles/globals.css';
export function VoiceWindow() {
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const microphone = useRef<MediaStream | null>(null);
  const microphoneGeneration = useRef(0);
  const stopMicrophone = () => {
    microphoneGeneration.current += 1;
    microphone.current?.getTracks().forEach((track) => track.stop());
    microphone.current = null;
  };
  const startMicrophone = async () => {
    stopMicrophone();
    const generation = microphoneGeneration.current;
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      if (generation !== microphoneGeneration.current) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }
      microphone.current = stream;
    } catch {
      setError('마이크를 시작하지 못했습니다. Windows와 ACE의 마이크 권한을 확인해 주세요.');
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
      stopMicrophone();
      setText('');
      setError('');
    });
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
      stopMicrophone();
      setText('');
    };
  }, []);
  const finish = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError('');
    try {
      const transient = text.trim();
      setText('');
      stopMicrophone();
      await invoke('submit_voice_command', { text: transient });
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
        onClose={() => {
          stopMicrophone();
          setText('');
          void invoke('hide_voice_overlay');
        }}
      />
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
