import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/performance',
  workers: 1,
  use: {
    channel: 'chrome',
    baseURL: 'http://127.0.0.1:4179',
    serviceWorkers: 'block',
  },
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1 --port 4179 --strictPort',
    url: 'http://127.0.0.1:4179',
    reuseExistingServer: process.env.PERF_REUSE_SERVER === '1',
    env: {
      VITE_API_BASE_URL: 'http://127.0.0.1:4179/api/',
      VITE_PUSHER_KEY: 'browser-test',
      VITE_PUSHER_HOST: '127.0.0.1',
    },
  },
});
