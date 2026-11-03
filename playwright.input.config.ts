import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  testMatch: ['input-window.spec.ts', 'orb-experience.spec.ts', 'native-safety.spec.ts'],
  workers: 1,
  use: {
    baseURL: 'http://127.0.0.1:1433',
    reducedMotion: 'reduce',
    launchOptions: {
      executablePath:
        process.env.PLAYWRIGHT_BROWSER_PATH ||
        'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
    },
  },
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1 --port 1433',
    url: 'http://127.0.0.1:1433',
    reuseExistingServer: false,
    env: { VITE_API_BASE_URL: 'http://127.0.0.1:3002' },
  },
});
