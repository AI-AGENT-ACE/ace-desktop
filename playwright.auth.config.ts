import { defineConfig } from '@playwright/test';
import baseline from './playwright.config';

// Authentication UI fault tests do not require a running database.
export default defineConfig({
  testDir: './tests',
  workers: 1,
  use: baseline.use,
  testMatch: 'auth.spec.ts',
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1 --port 1430',
    url: 'http://127.0.0.1:1430',
    reuseExistingServer: false,
    env: { VITE_API_BASE_URL: 'http://127.0.0.1:3002' },
  },
});
