import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const watchAnalytics = async (page) => {
  const requests = [];
  await page.route('https://analytics.dzik.email/**', (route) => {
    requests.push(route.request().url());
    return route.fulfill({ contentType: 'text/javascript', body: '' });
  });
  return requests;
};

for (const path of [
  '/login/github?code=x&state=y',
  '/login',
  '/register',
  '/desktop-auth/return',
  '/password/reset?token=t',
  '/email/verify?token=t',
  '/auth/link-confirm?token=t',
  '/workspaces/invitations/secret-token',
]) {
  test(`auth page ${path} does not load analytics`, async ({ page }) => {
    const requests = await watchAnalytics(page);
    await page.goto(path, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(1500);
    expect(requests).toEqual([]);
  });
}

for (const path of ['/', '/tmgrdev/board']) {
  test(`app page ${path} loads analytics`, async ({ page }) => {
    const requests = await watchAnalytics(page);
    await page.goto(path, { waitUntil: 'domcontentloaded' });
    await expect.poll(() => requests).toEqual([
      'https://analytics.dzik.email/js/script.js',
    ]);
  });
}

const watchGoogle = async (page) => {
  const requests = [];
  await page.addInitScript(() =>
    localStorage.setItem(
      'tmgr:consent:v1',
      JSON.stringify({ analytics: 'granted', updatedAt: '2026-10-01T00:00:00Z' }),
    ),
  );
  await page.route('https://www.googletagmanager.com/**', (route) => {
    requests.push(route.request().url());
    return route.fulfill({ contentType: 'text/javascript', body: '' });
  });
  return requests;
};

test('with analytics consent Google Analytics stays off the OAuth callback page', async ({ page }) => {
  const requests = await watchGoogle(page);
  await page.goto('/login/github?code=x&state=y', { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(1500);
  expect(requests).toEqual([]);
  expect(await page.evaluate(() => JSON.stringify(window.dataLayer ?? []))).not.toContain('code=x');
});

test('with analytics consent Google Analytics loads on an app page', async ({ page }) => {
  await mockApp(page);
  const requests = await watchGoogle(page);
  await page.goto('/tmgrdev/board', { waitUntil: 'domcontentloaded' });
  await expect.poll(() => requests.length).toBe(1);
  expect(requests[0]).toContain('https://www.googletagmanager.com/gtag/js?id=');
});

test('an in-app navigation to a token URL does a full load so Plausible stays off it', async ({ page }) => {
  await mockApp(page);
  const requests = await watchAnalytics(page);
  await page.goto('/tmgrdev/board', { waitUntil: 'domcontentloaded' });
  await expect.poll(() => requests.length).toBe(1);
  await page.evaluate(() => (window.__sameDocument = true));
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$router.push('/workspaces/invitations/secret-token'),
  );
  await expect(page).toHaveURL(/\/workspaces\/invitations\/secret-token$/);
  await expect.poll(() => page.evaluate(() => window.__sameDocument)).toBeUndefined();
  await page.waitForTimeout(1000);
  expect(requests).toHaveLength(1);
});
