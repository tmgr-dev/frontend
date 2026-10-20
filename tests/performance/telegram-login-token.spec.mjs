import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const watchTelegram = async (page, response) => {
  const authHeaders = [];
  page.on('request', (request) =>
    authHeaders.push(request.headers().authorization ?? ''),
  );
  await mockApp(page, { account: false });
  const bodies = [];
  await page.route('**/api/auth/login/telegram/redirect', async (route) => {
    bodies.push(route.request().postDataJSON());
    await route.fulfill(response);
  });
  return { authHeaders, bodies };
};

const storedToken = (page) =>
  page.evaluate(() => localStorage.getItem('token'));

test('a token in the Telegram callback URL is ignored', async ({ page }) => {
  const { authHeaders, bodies } = await watchTelegram(page, {
    status: 403,
    json: { message: 'Invalid or expired code.' },
  });

  await page.goto('/login/telegram?token=evil', {
    waitUntil: 'domcontentloaded',
  });

  await expect.poll(() => bodies.length).toBe(1);
  expect(bodies[0].code).toBeUndefined();
  await expect(page.getByText('Something went wrong')).toBeVisible();
  await expect(page).toHaveURL(/\/login$/, { timeout: 10_000 });
  expect(await storedToken(page)).toBeNull();
  expect(authHeaders.filter((h) => h.includes('evil'))).toEqual([]);
});

test('the Telegram one-time code is redeemed for a session', async ({
  page,
}) => {
  const { bodies } = await watchTelegram(page, {
    json: { data: { token: 'from-code' } },
  });

  await page.goto('/login/telegram?code=abc', {
    waitUntil: 'domcontentloaded',
  });

  await expect.poll(() => bodies).toEqual([{ code: 'abc' }]);
  await expect
    .poll(() => storedToken(page))
    .toBe(JSON.stringify({ token: 'from-code' }));
});
