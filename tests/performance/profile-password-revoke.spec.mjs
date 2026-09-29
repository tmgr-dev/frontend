import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const notice =
  'Device, agent and notification tokens were revoked — generate new ones.';

async function openProfile(page) {
  const puts = [];
  await mockApp(page);
  await page.route('**/api/user', async (route) => {
    const request = route.request();
    const user = {
      id: 1,
      name: 'Test User',
      email: 'test@example.test',
      settings: [{ key: 'current_workspace', value: '1' }],
      has_smart_device_token: true,
    };
    if (request.method() === 'PUT') {
      const body = request.postDataJSON();
      puts.push(body);
      return route.fulfill({
        json: {
          data: body.password
            ? { ...user, has_smart_device_token: false, smart_device_token: null }
            : user,
        },
      });
    }
    return route.fulfill({ json: { data: user } });
  });
  await page.goto('/settings?tab=profile');
  await expect(page.getByLabel('Name')).toBeVisible();
  return puts;
}

test('changing the password shows the revoked-tokens notice and clears the fields', async ({
  page,
}) => {
  const puts = await openProfile(page);

  await page.getByLabel('New password', { exact: true }).fill('new-secret-1');
  await page.getByLabel('Confirm password').fill('new-secret-1');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect(page.getByTestId('tokens-revoked-notice')).toHaveText(notice);
  await expect(page.getByLabel('New password', { exact: true })).toHaveValue('');
  await expect(page.getByLabel('Confirm password')).toHaveValue('');

  await page.getByLabel('Name').fill('Renamed User');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect.poll(() => puts.length).toBe(2);
  expect(puts[1].password ?? null).toBeNull();
});

test('a name-only save shows no revoked-tokens notice', async ({ page }) => {
  const puts = await openProfile(page);

  await page.getByLabel('Name').fill('Renamed User');
  await page.getByRole('button', { name: 'Save' }).click();

  await expect.poll(() => puts.length).toBe(1);
  await expect(page.getByText('User data saved')).toBeVisible();
  await expect(page.getByTestId('tokens-revoked-notice')).toHaveCount(0);
});
