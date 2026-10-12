import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/performance',
  workers: 1,
  projects: [
    { name: 'chrome', use: { channel: 'chrome' }, testIgnore: /desktop-csp/ },
    {
      name: 'webkit',
      use: { browserName: 'webkit' },
      testMatch: /desktop-csp/,
    },
  ],
  use: {
    baseURL: 'http://127.0.0.1:4179',
    serviceWorkers: 'block',
    storageState: {
      cookies: [],
      origins: [
        {
          origin: 'http://127.0.0.1:4179',
          localStorage: [
            {
              name: 'tmgr:consent:v1',
              value: JSON.stringify({
                analytics: 'denied',
                usage: 'denied',
                usageGeneration: 1,
                updatedAt: null,
              }),
            },
          ],
        },
      ],
    },
  },
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1 --port 4179 --strictPort',
    url: 'http://127.0.0.1:4179',
    reuseExistingServer: process.env.PERF_REUSE_SERVER === '1',
    env: {
      VITE_API_BASE_URL: 'http://127.0.0.1:4179/api/',
      VITE_PUSHER_KEY: 'browser-test',
      VITE_PUSHER_HOST: '127.0.0.1',
      VITE_TELEGRAM_BOT_NAME: 'tmgr_test_bot',
    },
  },
});
