import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test('password reset asks whether to sign out all devices', async ({
  page,
}) => {
  const posts = [];
  await page.route('**/api/password/reset/**', (route) => {
    posts.push(route.request().postDataJSON());
    return route.fulfill({ json: { message: 'ok' } });
  });
  await page.goto('/password/reset?token=abc');
  const box = page.getByTestId('logout-all-sessions');
  await expect(box).toBeVisible();
  await expect(box).toBeChecked();
  await expect(page.getByLabel('Sign out of all devices')).toBeChecked();

  await page.locator('#password').fill('new-secret-1');
  await page.locator('#password_confirmation').fill('new-secret-1');
  await page.getByRole('button', { name: 'Reset' }).click();
  await expect.poll(() => posts.length).toBe(1);
  expect(posts[0].logout_all_sessions).toBe(true);

  await box.uncheck();
  await page.getByRole('button', { name: 'Reset' }).click();
  await expect.poll(() => posts.length).toBe(2);
  expect(posts[1].logout_all_sessions).toBe(false);
});

async function openProfile(page) {
  const puts = [];
  await mockApp(page);
  await page.addInitScript(() =>
    localStorage.setItem(
      'token',
      JSON.stringify({ token: 'fixture', refresh_token: 'refresh-fixture' }),
    ),
  );
  await page.route('**/api/user', async (route) => {
    const request = route.request();
    const user = {
      id: 1,
      name: 'Test User',
      email: 'test@example.test',
      settings: [{ key: 'current_workspace', value: '1' }],
    };
    if (request.method() === 'PUT') {
      puts.push(request.postDataJSON());
    }
    return route.fulfill({ json: { data: user } });
  });
  await page.goto('/settings?tab=profile');
  await expect(page.getByLabel('Name')).toBeVisible();
  return puts;
}

test('profile password change sends the sign-out-other-devices choice', async ({
  page,
}) => {
  const puts = await openProfile(page);
  const box = page.getByTestId('logout-other-sessions');
  await expect(box).toBeVisible();
  await expect(box).toBeChecked();

  await page.getByLabel('New password', { exact: true }).fill('new-secret-1');
  await page.getByLabel('Confirm password').fill('new-secret-1');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect.poll(() => puts.length).toBe(1);
  expect(puts[0].logout_other_sessions).toBe(true);
  expect(puts[0].current_refresh_token).toBe('refresh-fixture');

  await box.uncheck();
  await page.getByLabel('New password', { exact: true }).fill('new-secret-2');
  await page.getByLabel('Confirm password').fill('new-secret-2');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect.poll(() => puts.length).toBe(2);
  expect(puts[1].logout_other_sessions).toBe(false);
});

test('a name-only profile save sends no session fields', async ({ page }) => {
  const puts = await openProfile(page);
  await page.getByLabel('Name').fill('Renamed User');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect.poll(() => puts.length).toBe(1);
  expect(puts[0]).not.toHaveProperty('logout_other_sessions');
  expect(puts[0]).not.toHaveProperty('current_refresh_token');
});
