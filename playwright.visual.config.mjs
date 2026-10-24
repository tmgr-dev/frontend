import { defineConfig } from '@playwright/test';
import os from 'node:os';
import path from 'node:path';

// Visual regression (TM-209). Not part of CI or the default config: baselines are macOS + system Chrome.
// Capture: VISUAL_DIST=<dist> npx playwright test -c playwright.visual.config.mjs --update-snapshots
// Compare: VISUAL_DIST=<other dist> npx playwright test -c playwright.visual.config.mjs
// VISUAL_SNAPSHOTS_DIR / VISUAL_OUTPUT_DIR keep PNGs outside the repo.
// Build both dists with the same env (as in playwright.config.mjs), then copy dist/ out:
// VITE_API_BASE_URL=http://127.0.0.1:4179/api/ VITE_PUSHER_KEY=browser-test VITE_PUSHER_HOST=127.0.0.1 VITE_TELEGRAM_BOT_NAME=tmgr_test_bot npx vite build
const root = process.env.VISUAL_ROOT || path.join(os.tmpdir(), 'tmgr-visual');
const snapshots =
  process.env.VISUAL_SNAPSHOTS_DIR || path.join(root, 'snapshots');
const output = process.env.VISUAL_OUTPUT_DIR || path.join(root, 'results');
const dist = process.env.VISUAL_DIST;
const reuse = process.env.VISUAL_REUSE_SERVER === '1';
if (!dist && !reuse)
  throw new Error(
    'Set VISUAL_DIST=<built dist dir> (or VISUAL_REUSE_SERVER=1 with a server on 127.0.0.1:4179)',
  );

const storageState = {
  cookies: [],
  origins: [
    {
      origin: 'http://127.0.0.1:4179',
      localStorage: [
        {
          name: 'tmgr:consent:v1',
          value: JSON.stringify({ analytics: 'denied', updatedAt: null }),
        },
        {
          name: 'tmgr:telemetry:consent',
          value: JSON.stringify({ on: false, generation: 1 }),
        },
      ],
    },
  ],
};

// Software raster + fixed colour profile: GPU raster left 2-50px anti-aliasing noise on icons and gradients.
const chrome = {
  channel: 'chrome',
  launchOptions: {
    args: [
      '--disable-gpu',
      '--force-color-profile=srgb',
      '--disable-partial-raster',
      '--disable-skia-runtime-opts',
      '--disable-lcd-text',
    ],
  },
};
const desktop = { viewport: { width: 1440, height: 900 } };
const mobile = {
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 2,
  isMobile: true,
  hasTouch: true,
};

export default defineConfig({
  testDir: './tests/visual',
  outputDir: output,
  snapshotPathTemplate: `${snapshots}/{projectName}/{testFileName}/{arg}{ext}`,
  workers: 1,
  timeout: 90_000,
  reporter: [
    ['list'],
    ['json', { outputFile: path.join(output, 'report.json') }],
  ],
  expect: {
    toHaveScreenshot: {
      animations: 'disabled',
      caret: 'hide',
      scale: 'device',
      maxDiffPixels: 0,
      // 0: any colour change counts. The default 0.2 hid a 16px->6px corner on white-on-grey cards.
      threshold: 0,
    },
  },
  projects: [
    { name: 'desktop', use: { ...chrome, ...desktop } },
    { name: 'mobile', use: { ...chrome, ...mobile } },
    {
      name: 'webkit-desktop',
      use: { browserName: 'webkit', ...desktop },
      grep: /@webkit/,
    },
  ],
  use: {
    baseURL: 'http://127.0.0.1:4179',
    serviceWorkers: 'block',
    locale: 'en-US',
    timezoneId: 'UTC',
    colorScheme: 'light',
    reducedMotion: 'reduce',
    storageState,
  },
  webServer: {
    command: `npx vite preview --outDir "${dist}" --host 127.0.0.1 --port 4179 --strictPort`,
    url: 'http://127.0.0.1:4179',
    reuseExistingServer: reuse,
  },
});
