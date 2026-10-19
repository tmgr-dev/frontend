import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

/** Logged-out visitor whose every API call fails; the widget script is a no-op. */
const loggedOut = async (page) => {
  const authHeaders = [];
  page.on('request', (request) =>
    authHeaders.push(request.headers().authorization ?? ''),
  );
  const apiRequests = [];
  await mockApp(page, { account: false, apiRequests });
  await page.route('https://telegram.org/**', (route) =>
    route.fulfill({ contentType: 'text/javascript', body: '' }),
  );
  return { authHeaders, apiRequests };
};

const storedToken = (page) =>
  page.evaluate(() => localStorage.getItem('token'));

test('an old Telegram code link lands on sign-in without any auth request', async ({
  page,
}) => {
  const { apiRequests } = await loggedOut(page);

  await page.goto('/login/telegram?code=abc');

  await expect(page).toHaveURL(/\/login$/);
  await page.waitForTimeout(300);
  expect(apiRequests.filter((r) => /\/api\/(auth|user)/.test(r))).toEqual([]);
  expect(await storedToken(page)).toBeNull();
});

for (const platform of ['github', 'google', 'apple', 'telegram', 'unknown']) {
  test(`a token in the ${platform} callback URL is ignored`, async ({
    page,
  }) => {
    const { authHeaders } = await loggedOut(page);

    await page.goto(`/login/${platform}?token=evil`);

    await expect(page).toHaveURL(/\/login$/, { timeout: 10_000 });
    expect(await storedToken(page)).toBeNull();
    expect(authHeaders.filter((h) => h.includes('evil'))).toEqual([]);
  });
}
