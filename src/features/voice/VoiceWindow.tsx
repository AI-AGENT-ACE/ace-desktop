import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { VoiceOverlay } from './VoiceOverlay';
import { apiErrorMessage } from '../../api/client';
import '../../styles/globals.css';
export function VoiceWindow() {
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    document.documentElement.dataset.theme = localStorage.getItem('ace-theme') || 'light';
  }, []);
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    listen('ace-voice-reset', () => {
      setText('');
      setError('');
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    return () => {
      disposed = true;
      stop?.();
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
          setText('');
          void invoke('hide_voice_overlay');
        }}
      />
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
