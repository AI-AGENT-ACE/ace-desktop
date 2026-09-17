import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { VoiceWindow } from './features/voice/VoiceWindow';
import './styles/api.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>{location.hash === '#voice' ? <VoiceWindow /> : <App />}</React.StrictMode>,
);
