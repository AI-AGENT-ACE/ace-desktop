import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  testMatch: 'voice-regression.spec.ts',
  workers: 1,
  use: { baseURL: 'http://127.0.0.1:1431', headless: true },
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1 --port 1431',
    url: 'http://127.0.0.1:1431',
    reuseExistingServer: false,
    env: { VITE_API_BASE_URL: 'http://127.0.0.1:3002' },
  },
});
