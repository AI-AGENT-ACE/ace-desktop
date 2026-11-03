import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  testMatch: 'orb-experience.spec.ts',
  workers: 1,
  use: {
    baseURL: 'http://127.0.0.1:1432',
    reducedMotion: 'reduce',
    launchOptions: {
      executablePath:
        process.env.PLAYWRIGHT_BROWSER_PATH ||
        'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
    },
  },
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1 --port 1432',
    url: 'http://127.0.0.1:1432',
    reuseExistingServer: false,
    env: { VITE_API_BASE_URL: 'http://127.0.0.1:3002' },
  },
});
