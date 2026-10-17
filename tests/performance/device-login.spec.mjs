import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { mockApp } from './mockApp.mjs';

const shots = process.env.DEVICE_SHOTS;
const shot = async (page, name) => {
  if (!shots) return;
  mkdirSync(shots, { recursive: true });
  await page.screenshot({ path: `${shots}/${name}.png`, fullPage: true });
};

const request = {
  client_name: 'tmgr-cli',
  host_label: '<b>work-laptop</b>',
  requested_scopes: [
    'workspaces:read',
    'tasks:read',
    'tasks:write',
    'tasks:create',
    'comments:create',
    'time:read',
    'time:write',
  ],
  created_at: '2026-10-12T10:00:00Z',
  expires_at: '2026-10-12T10:10:00Z',
};

async function mockDevice(page, { found = true } = {}) {
  const calls = [];
  await page.route('**/api/auth/device/requests/**', async (route) => {
    const req = route.request();
    const path = new URL(req.url()).pathname;
    calls.push({
      method: req.method(),
      path,
      body: req.method() === 'POST' ? req.postDataJSON() : null,
    });
    if (req.method() === 'GET') {
      return found
        ? route.fulfill({ json: { data: request } })
        : route.fulfill({ status: 404, json: { error: 'not_found' } });
    }
    return route.fulfill({ status: 204, body: '' });
  });
  return calls;
}

test('prefilled code shows the request and approves the checked scopes', async ({
  page,
}) => {
  await mockApp(page);
  const calls = await mockDevice(page);
  await page.goto('/device?code=abcd2345');

  await expect(page.getByTestId('device-client')).toHaveText('tmgr-cli');
  await expect(page.getByTestId('device-host')).toHaveText(
    '<b>work-laptop</b>',
  );
  await expect(page.getByTestId('device-host').locator('b')).toHaveCount(0);
  expect(calls[0].path).toBe('/api/auth/device/requests/ABCD-2345');
  await expect(page.locator('input[type="checkbox"]:checked')).toHaveCount(7);
  await expect(page.getByText('Only approve if you started')).toBeVisible();
  await shot(page, '01-request');

  await page.getByTestId('scope-time:write').uncheck();
  await page.getByRole('button', { name: 'Approve' }).click();
  await expect(
    page.getByText('Device approved. You can return to your terminal.'),
  ).toBeVisible();
  const approve = calls.find((c) => c.path.endsWith('/approve'));
  expect(approve.body.scopes).not.toContain('time:write');
  expect(approve.body.scopes).toHaveLength(6);
  await shot(page, '02-approved');
});

test('approve is disabled with nothing selected and deny posts deny', async ({
  page,
}) => {
  await mockApp(page);
  const calls = await mockDevice(page);
  await page.goto('/device?code=ABCD-2345');
  await expect(page.getByTestId('device-client')).toBeVisible();
  for (const scope of request.requested_scopes) {
    await page.getByTestId(`scope-${scope}`).uncheck();
  }
  await expect(page.getByRole('button', { name: 'Approve' })).toBeDisabled();
  await shot(page, '03-none-selected');
  await page.getByRole('button', { name: 'Deny' }).click();
  await expect(
    page.getByText('Request denied. Nothing was signed in.'),
  ).toBeVisible();
  expect(calls.some((c) => c.path.endsWith('/deny'))).toBe(true);
});

test('manual entry normalises the code; unknown code shows the expired message', async ({
  page,
}) => {
  await mockApp(page);
  const calls = await mockDevice(page, { found: false });
  await page.goto('/device');
  await page.getByLabel('Code').fill('wxyz9876');
  await expect(page.getByLabel('Code')).toHaveValue('WXYZ-9876');
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(
    page.getByText('This code is invalid or has expired.'),
  ).toBeVisible();
  expect(calls[0].path).toBe('/api/auth/device/requests/WXYZ-9876');
  await shot(page, '04-invalid');
});

test('too many attempts shows the Retry-After wait', async ({ page }) => {
  await mockApp(page);
  await page.route('**/api/auth/device/requests/**', (route) =>
    route.fulfill({
      status: 429,
      headers: { 'Retry-After': '420' },
      json: { error: 'too_many_attempts' },
    }),
  );
  await page.goto('/device?code=ABCD-2345');
  await expect(
    page.getByText('Too many wrong codes. Try again in 7 minutes.'),
  ).toBeVisible();
});

test('layout is just the card in the app shell', async ({ page }) => {
  await mockApp(page);
  await mockDevice(page);
  await page.goto('/device?code=ABCD-2345');
  await expect(page.getByTestId('device-client')).toBeVisible();
  await expect(page.getByText('Plan the work.')).toHaveCount(0);
});

test('logged out: login then come back to /device with the code', async ({
  page,
}) => {
  await mockApp(page, { account: false, loginAs: 'tok' });
  const calls = await mockDevice(page);
  await page.goto('/device?code=ABCD-2345');
  await expect(page).toHaveURL(/\/login/);
  await page.locator('input[type="email"]').fill('test@example.test');
  await page.locator('input[type="password"]').fill('secret-pass');
  await page.locator('form button[type="submit"]').click();
  await expect(page).toHaveURL(/\/device\?code=ABCD-2345/);
  await expect(page.getByTestId('device-client')).toHaveText('tmgr-cli');
  expect(calls[0].path).toBe('/api/auth/device/requests/ABCD-2345');
});

const devices = [
  {
    id: 'dev-1',
    client_name: 'tmgr-cli',
    host_label: 'work-laptop',
    scopes: ['cli', 'tasks:read', 'tasks:write', 'time:write'],
    created_at: '2026-10-10T09:00:00Z',
    last_used_at: '2026-10-12T08:30:00Z',
  },
  {
    id: 'dev-2',
    client_name: 'tmgr-cli',
    host_label: 'ci-box',
    scopes: ['cli', 'tasks:read'],
    created_at: '2026-10-01T09:00:00Z',
    last_used_at: null,
  },
];

async function openProfile(page, list) {
  await mockApp(page);
  const calls = [];
  await page.route('**/api/auth/devices**', async (route) => {
    const req = route.request();
    calls.push({
      method: req.method(),
      path: new URL(req.url()).pathname,
      body: ['PATCH', 'POST'].includes(req.method())
        ? req.postDataJSON()
        : null,
    });
    if (req.method() === 'GET') return route.fulfill({ json: { data: list } });
    if (req.method() === 'PATCH') {
      const id = new URL(req.url()).pathname.split('/').pop();
      const device = list.find((d) => d.id === id);
      return route.fulfill({
        json: {
          data: { ...device, scopes: ['cli', ...req.postDataJSON().scopes] },
        },
      });
    }
    return route.fulfill({ status: 204, body: '' });
  });
  await page.goto('/settings?tab=profile');
  await expect(page.getByText('Signed-in devices')).toBeVisible();
  return calls;
}

test('devices list shows chips, edits scopes and revokes', async ({ page }) => {
  const calls = await openProfile(page, devices);
  await expect(page.getByTestId('device-dev-1')).toContainText('work-laptop');
  await expect(page.getByTestId('device-dev-1')).toContainText('tasks:write');
  await expect(page.getByTestId('device-dev-1')).not.toContainText('cli,');
  await shot(page, '05-devices');

  await page
    .getByTestId('device-dev-1')
    .getByRole('button', { name: 'Edit scopes' })
    .click();
  await page.getByTestId('scope-time:write').uncheck();
  await shot(page, '06-devices-edit');
  await page.getByRole('button', { name: 'Save' }).last().click();
  await expect.poll(() => calls.find((c) => c.method === 'PATCH')).toBeTruthy();
  expect(calls.find((c) => c.method === 'PATCH')).toMatchObject({
    path: '/api/auth/devices/dev-1',
    body: { scopes: ['tasks:read', 'tasks:write'] },
  });

  page.once('dialog', (dialog) => dialog.accept());
  await page
    .getByTestId('device-dev-2')
    .getByRole('button', { name: 'Revoke' })
    .click();
  await expect(page.getByTestId('device-dev-2')).toHaveCount(0);
  expect(
    calls.some((c) => c.method === 'DELETE' && c.path.endsWith('/dev-2')),
  ).toBe(true);
  await expect(page.getByTestId('device-dev-1')).not.toContainText(
    'time:write',
  );
});

test('empty devices list shows the hint', async ({ page }) => {
  await openProfile(page, []);
  await expect(page.getByTestId('devices-empty')).toContainText(
    'No CLI devices yet. Run tmgr login in your terminal.',
  );
  await shot(page, '07-devices-empty');
});
