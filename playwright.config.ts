import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  fullyParallel: false,
  workers: 1,
  use: {
    baseURL: 'http://127.0.0.1:1430',
    viewport: { width: 1200, height: 800 },
    launchOptions: {
      executablePath:
        process.env.PLAYWRIGHT_BROWSER_PATH ||
        'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
    },
  },
  webServer: [
    {
      command: 'node scripts/browser-test-server.mjs',
      cwd: '../ace-backend',
      url: 'http://127.0.0.1:3002/health',
      reuseExistingServer: false,
      timeout: 60000,
    },
    {
      command: 'npm run dev -- --host 127.0.0.1 --port 1430',
      url: 'http://127.0.0.1:1430',
      reuseExistingServer: false,
      env: { VITE_API_BASE_URL: 'http://127.0.0.1:3002' },
    },
  ],
});
