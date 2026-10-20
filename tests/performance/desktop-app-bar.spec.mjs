import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const MAC_UA =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36';
const IPHONE_UA =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1';
const SHOTS = process.env.BAR_SHOTS_DIR;

const seen = () => new Date().toISOString();

test.describe('desktop app bar', () => {
  test.use({ userAgent: MAC_UA });

  test('shows for an account that used the desktop app, links to the same page, hides on close', async ({
    page,
  }) => {
    await mockApp(page, { userExtra: { desktop_app_seen_at: seen() } });
    await page.goto('/demo/board');
    await page.waitForLoadState('networkidle');
    const bar = page.getByTestId('desktop-app-bar');
    await expect(bar).toBeVisible();
    await expect(bar.getByRole('link')).toHaveAttribute(
      'href',
      'tmgr://open?path=%2Fdemo%2Fboard',
    );
    if (SHOTS) await bar.screenshot({ path: `${SHOTS}/bar-light.png` });
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.evaluate(() => document.documentElement.classList.add('dark'));
    if (SHOTS) await bar.screenshot({ path: `${SHOTS}/bar-dark.png` });
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/page-dark.png` });
    await page.evaluate(() =>
      document.documentElement.classList.remove('dark'),
    );
    await page
      .getByRole('button', { name: /Hide this bar/ })
      .dispatchEvent('click');
    await expect(bar).toBeHidden();
    await page.reload();
    await expect(page.getByTestId('desktop-app-bar')).toBeHidden();
  });

  test('hidden without the desktop flag', async ({ page }) => {
    await mockApp(page, { userExtra: { desktop_app_seen_at: null } });
    await page.goto('/demo/board');
    await page.waitForLoadState('networkidle');
    await expect(page.getByTestId('desktop-app-bar')).toHaveCount(0);
  });
});

test.describe('mobile browser', () => {
  test.use({ userAgent: IPHONE_UA });

  test('hidden on iPhone even with the flag', async ({ page }) => {
    await mockApp(page, { userExtra: { desktop_app_seen_at: seen() } });
    await page.goto('/demo/board');
    await page.waitForLoadState('networkidle');
    await expect(page.getByTestId('desktop-app-bar')).toHaveCount(0);
  });
});
