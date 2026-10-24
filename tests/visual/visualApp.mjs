import { expect } from '@playwright/test';
import { mockApp } from '../performance/mockApp.mjs';

// Fixed "now" for every visual test: relative dates, greetings and calendars render the same each run.
export const NOW = '2026-09-15T10:00:00Z';

export const SCHEMES = {
  light: { colorScheme: 'default', theme: 'default' },
  dark: { colorScheme: 'dark', theme: 'default' },
  dracula: { colorScheme: 'dark', theme: 'dracula' },
};

export async function prepare(page, scheme = 'light') {
  const { colorScheme, theme } = SCHEMES[scheme];
  await page.clock.setFixedTime(NOW);
  await page.addInitScript(
    ([cs, th]) => {
      sessionStorage.setItem('tmgr:tours:auto', 'true');
      localStorage.setItem('colorScheme', cs);
      localStorage.setItem('theme', th);
    },
    [colorScheme, theme],
  );
  // Nothing outside the local preview server: analytics, avatars, widgets stay deterministic.
  await page.route(
    (url) => url.hostname !== '127.0.0.1',
    (route) => route.abort(),
  );
}

export async function boot(page, scheme, mockOptions = {}) {
  await prepare(page, scheme);
  return mockApp(page, { taskCount: 3, ...mockOptions });
}

// Settles fonts, network and layout before a screenshot.
export async function settle(page) {
  await page.waitForLoadState('networkidle');
  await page.evaluate(() => document.fonts.ready);
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
}

export async function shoot(page, name, { fullPage = true, mask = [] } = {}) {
  await settle(page);
  await expect(page).toHaveScreenshot(`${name}.png`, { fullPage, mask });
}
