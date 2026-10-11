import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const screensDir = process.env.SCREENS_DIR;
if (screensDir) mkdirSync(screensDir, { recursive: true });
const shot = async (page, name) => {
  if (screensDir) await page.screenshot({ path: join(screensDir, name) });
};

test.use({ viewport: { width: 1280, height: 800 } });

const localPage = async (page, { profile = null, dark = false } = {}) => {
  const apiRequests = [];
  const shell = await desktopPage(
    page,
    { account: false, apiRequests },
    { profile },
  );
  await page.addInitScript((scheme) => {
    localStorage.setItem('colorScheme', scheme);
  }, dark ? 'dark' : 'light');
  return { shell, apiRequests };
};

const profile = {
  id: -1,
  uuid: '00000000-0000-4000-8000-000000000001',
  name: 'Ada',
  created_at: '2026-10-11T10:00:00Z',
};

const startLocal = async (page) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/welcome/);
  await page.getByTestId('start-local').click();
  await page.getByLabel('Your name').fill('Ada');
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
};

const addTask = async (page, title) => {
  await page.getByRole('button', { name: 'Open Add Task Modal' }).first().click();
  await page.getByPlaceholder('Task name').fill(title);
  await page.keyboard.press('ControlOrMeta+KeyS');
};

test('first run: welcome, start without an account, create a task', async ({
  page,
}) => {
  const { shell, apiRequests } = await localPage(page);
  await page.goto('/');
  await expect(page).toHaveURL(/\/welcome/);
  await expect(
    page.getByRole('button', { name: 'Start without an account' }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
  await shot(page, 'welcome.png');

  await page.getByTestId('start-local').click();
  await page.getByLabel('Your name').fill('Ada');
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  expect(shell.profile.name).toBe('Ada');

  await page.goto('/local-my-workspace/board');
  await addTask(page, 'First local task');
  await expect
    .poll(async () =>
      (
        await shell('local_db_select', {
          code: 'my-workspace',
          sql: 'SELECT title FROM tasks',
          params: [],
        })
      ).map((row) => row.title),
    )
    .toContain('First local task');
  await page.goto('/local-my-workspace/list');
  await expect(page.getByText('First local task').first()).toBeVisible();
  await page.goto('/local-my-workspace/board');
  await expect(page.getByText('First local task').first()).toBeVisible();
  await expect(page.getByTestId('local-only-banner')).toBeVisible();
  await shot(page, 'local-board.png');
  expect(apiRequests).toEqual([]);
});

test('welcome in dark mode', async ({ page }) => {
  await localPage(page, { dark: true });
  await page.goto('/');
  await expect(page).toHaveURL(/\/welcome/);
  await page.getByTestId('start-local').click();
  await expect(page.getByTestId('start-form')).toBeVisible();
  await shot(page, 'welcome-dark.png');
});

test('reload keeps the profile and the task', async ({ page }) => {
  const { apiRequests } = await localPage(page);
  await startLocal(page);
  await page.goto('/local-my-workspace/board');
  await addTask(page, 'Survives reload');
  await expect(page.getByText('Survives reload').first()).toBeVisible();
  await page.reload();
  await expect(page).not.toHaveURL(/\/welcome/);
  await page.goto('/local-my-workspace/list');
  await expect(page.getByText('Survives reload').first()).toBeVisible();
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  expect(apiRequests).toEqual([]);
});

test('cloud-only modules and pages require an account', async ({ page }) => {
  const { apiRequests } = await localPage(page, { profile });
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await page.goto('/settings/modules');
  await expect(page.getByTestId('requires-account').first()).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Create account' }).first(),
  ).toBeVisible();
  for (const name of ['Dashboard', 'Pomodoro'])
    await expect(page.getByText(name, { exact: true }).first()).toBeVisible();
  await shot(page, 'requires-account-modules.png');

  await page.goto('/local-my-workspace/dashboard');
  await expect(page.getByTestId('requires-account')).toBeVisible();
  await expect(page.getByText('Requires an account')).toBeVisible();
  await shot(page, 'requires-account-dashboard.png');
  expect(apiRequests).toEqual([]);
});

test('local data tab and local agents', async ({ page }) => {
  const { shell, apiRequests } = await localPage(page, { profile });
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await page.goto('/settings?tab=data');
  await expect(page.getByText('No backup yet')).toBeVisible();
  const backUp = page.getByRole('button', { name: /Back up now/i });
  await expect(backUp).toBeVisible();
  await backUp.click();
  await expect(page.getByText('No backup yet')).toHaveCount(0);
  await expect(page.getByText(/^Last backup/).first()).toBeVisible();
  expect(shell.unknown).not.toContain('local_backup_now');
  await shot(page, 'local-data.png');

  await page.goto('/settings/personas');
  await expect(page.getByText('Local agent').first()).toBeVisible();
  await shot(page, 'local-agents.png');
  expect(apiRequests).toEqual([]);
});

test('web: no desktop stub goes to login, never welcome', async ({ page }) => {
  await page.routeWebSocket(/.*/, () => {});
  await page.route('**/api/**', (route) =>
    route.fulfill({ status: 401, json: { message: 'Unauthenticated' } }),
  );
  await page.goto('/');
  await expect(page).toHaveURL(/\/login/);
  expect(page.url()).not.toContain('welcome');
});

test('cloud: desktop stub with a token still boots the cloud workspace', async ({
  page,
}) => {
  const requests = [];
  page.on('request', (request) => {
    const url = new URL(request.url());
    if (url.pathname.startsWith('/api/')) requests.push(url.pathname);
  });
  await desktopPage(page);
  await page.goto('/');
  await expect(page).not.toHaveURL(/\/welcome/);
  await expect(page).toHaveURL(/\/demo\//);
  expect(requests.length).toBeGreaterThan(0);
});
