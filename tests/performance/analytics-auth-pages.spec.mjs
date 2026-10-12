import { expect, test } from '@playwright/test';

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
