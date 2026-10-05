import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { VoiceWindow } from './features/voice/VoiceWindow';
import './styles/api.css';
import { isTauri } from '@tauri-apps/api/core';
import { serveVoiceSession } from './api/voice-session';

if (isTauri() && location.hash !== '#voice') {
  void serveVoiceSession().then((stop) => {
    if (import.meta.hot) import.meta.hot.dispose(stop);
  });
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>{location.hash === '#voice' ? <VoiceWindow /> : <App />}</React.StrictMode>,
);
