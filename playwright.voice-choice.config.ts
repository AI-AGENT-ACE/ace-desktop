import { defineConfig } from '@playwright/test';
import base from './playwright.config';

// Pure speech-choice parsing needs no backend, database, microphone or OS action.
export default defineConfig({
  ...base,
  testMatch: [
    'voice-choice-regression.spec.ts',
    'chat-controls-regression.spec.ts',
    'native-safety.spec.ts',
  ],
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1 --port 1430',
    url: 'http://127.0.0.1:1430',
    reuseExistingServer: false,
    env: { VITE_API_BASE_URL: 'http://127.0.0.1:3002' },
  },
});
