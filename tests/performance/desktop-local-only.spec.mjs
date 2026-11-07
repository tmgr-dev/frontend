import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { desktopPage } from './desktopShell.mjs';

const screensDir = process.env.SCREENS_DIR;
if (screensDir) mkdirSync(screensDir, { recursive: true });
const shot = async (page, name) => {
  if (screensDir) await page.screenshot({ path: join(screensDir, name) });
};

test.use({ viewport: { width: 1280, height: 800 } });

const localPage = async (
  page,
  {
    profile = null,
    dark = false,
    loginAs = null,
    registerOutcomes = [],
    upgrade = {},
  } = {},
) => {
  const apiRequests = [];
  const authHeaders = [];
  const shell = await desktopPage(
    page,
    { account: false, apiRequests, loginAs, authHeaders, registerOutcomes },
    { profile, upgrade },
  );
  await page.addInitScript(
    (scheme) => {
      localStorage.setItem('colorScheme', scheme);
    },
    dark ? 'dark' : 'light',
  );
  return { shell, apiRequests, authHeaders };
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
  await page
    .getByRole('button', { name: 'New task', exact: true })
    .first()
    .click();
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

test('settings opened directly as the first page keeps the pages tree', async ({
  page,
}) => {
  const { apiRequests } = await localPage(page, { profile });
  await page.goto('/settings/modules');
  await expect(page.getByTestId('requires-account').first()).toBeVisible();
  await expect(page.getByTestId('pages-sidebar-section')).toBeVisible();
  await expect(page.getByText('Failed to load pages')).toHaveCount(0);
  await expect(page.getByText('Loading…')).toHaveCount(0);
  await shot(page, 'modules-direct.png');
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

test('sign in from local-only mode keeps local workspaces next to the cloud one', async ({
  page,
}) => {
  const { shell, apiRequests, authHeaders } = await localPage(page, {
    profile,
    loginAs: 'cloud-token',
  });
  const created = await shell('local_workspace_create', {
    name: 'My workspace',
  });
  await shell('local_workspace_set_schema', {});
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await addTask(page, 'Local only task');
  await expect(page.getByText('Local only task').first()).toBeVisible();
  expect(created.code).toBe('my-workspace');
  await page.keyboard.press('Escape');
  expect(apiRequests).toEqual([]);

  await page.getByRole('button', { name: 'Create account' }).first().click();
  await expect(page).toHaveURL(/\/register/);
  await page.getByRole('link', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/login/);
  await page.locator('input[type="email"]').fill('test@example.test');
  await page.locator('input[type="password"]').fill('secret-pass');
  await page.locator('form button[type="submit"]').click();

  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await page
    .getByRole('button', { name: 'Stay in the local workspace' })
    .click();
  await expect(page.getByText('Local only task').first()).toBeVisible();
  await expect.poll(() => authHeaders.length).toBeGreaterThan(0);
  expect(authHeaders.every((h) => h === 'Bearer cloud-token')).toBe(true);
  await expect(page.getByText('Test User').first()).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Create account' }),
  ).toHaveCount(0);

  await page.getByTitle('Switch workspace').first().click();
  await expect(
    page.getByRole('menuitem', { name: /My workspace/ }),
  ).toBeVisible();
  await page.getByRole('menuitem', { name: /Demo/ }).click();
  await expect(page).toHaveURL(/\/demo\//);
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /My workspace/ }).click();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await expect(page.getByText('Local only task').first()).toBeVisible();
});

const fillRegister = async (page, { confirm = 'secret-pass' } = {}) => {
  await page.locator('#name').fill('Ada');
  await page.locator('#email').fill('ada@example.test');
  await page.locator('#password').fill('secret-pass');
  await page.locator('#password_confirmation').fill(confirm);
  await page.locator('form button[type="submit"]').click();
};

const markerOf = (page) =>
  page.evaluate(() => localStorage.getItem('tmgr:local-upgrade'));

const upgradeDialog = (page) =>
  page.getByRole('dialog', { name: 'Your account is ready' });

const SENTINEL_KEY = 'SENTINEL-PROVIDER-KEY-VALUE';
const SENTINEL_WORKSPACE = 'SENTINEL-WS-ALPHA';

const withLocalWorkspace = async (page, options) => {
  const ctx = await localPage(page, options);
  await ctx.shell('local_workspace_create', { name: SENTINEL_WORKSPACE });
  await ctx.shell('local_workspace_set_schema', {});
  return ctx;
};

test('sign up from local mode keeps local workspaces next to the cloud one and shows the dialog once', async ({
  page,
}) => {
  const { shell } = await withLocalWorkspace(page, {
    profile,
    loginAs: 'cloud-token',
  });
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-/);
  await page.getByRole('button', { name: 'Create account' }).first().click();
  await expect(page).toHaveURL(/\/register/);
  await fillRegister(page);

  const dialog = upgradeDialog(page);
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText('Your account is ready')).toBeVisible();
  await expect(page.getByTestId('local-upgrade-local')).toContainText(
    SENTINEL_WORKSPACE,
  );
  await expect(page.getByTestId('local-upgrade-cloud')).toContainText('Demo');
  await expect(page.getByTestId('local-upgrade-key')).toContainText(
    'Your AI provider key moved to this account.',
  );
  expect(shell.upgradeCalls.map((c) => c.command)).toEqual([
    'local_profile_link',
    'persona_cache_migrate_local',
    'persona_llm_migrate_local',
  ]);
  expect(shell.profile.linked_user_id).toBe(1);
  expect(
    shell.upgradeCalls.every(
      (c) => c.args.userId === 1 || c.args.toUserId === 1,
    ),
  ).toBe(true);

  await page.getByTestId('local-upgrade-stay').click();
  await expect(dialog).toHaveCount(0);
  await expect(page).toHaveURL(/\/local-/);
  expect(await markerOf(page)).toBeNull();
  await page.reload();
  await expect(page.getByTestId('sidebar-create-account')).toHaveCount(0);
  await expect(upgradeDialog(page)).toHaveCount(0);

  await page.getByTitle('Switch workspace').first().click();
  await expect(
    page.getByRole('menuitem', { name: new RegExp(SENTINEL_WORKSPACE) }),
  ).toBeVisible();
  await expect(page.getByRole('menuitem', { name: /Demo/ })).toBeVisible();
});

test('open the cloud workspace from the dialog', async ({ page }) => {
  await withLocalWorkspace(page, { profile, loginAs: 'cloud-token' });
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-/);
  await page.getByRole('button', { name: 'Create account' }).first().click();
  await fillRegister(page);
  await page.getByTestId('local-upgrade-open-cloud').click();
  await expect(page).toHaveURL(/\/demo\//);
  await expect(upgradeDialog(page)).toHaveCount(0);
  expect(await markerOf(page)).toBeNull();
});

test('failed and offline sign-up leave local mode intact and a retry works', async ({
  page,
}) => {
  const { apiRequests } = await withLocalWorkspace(page, {
    profile,
    loginAs: 'cloud-token',
    registerOutcomes: [
      { status: 409, json: { error: 'email_taken' } },
      { status: 429, json: { error: 'rate_limited' } },
      'abort',
      'ok',
    ],
  });
  const register = () =>
    apiRequests.filter((r) => r === 'POST /api/auth/register').length;
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-/);
  await page.getByRole('button', { name: 'Create account' }).first().click();
  await expect(page).toHaveURL(/\/register/);

  await fillRegister(page, { confirm: 'different' });
  await expect(page.getByText('does not match')).toBeVisible();
  expect(register()).toBe(0);

  await page.locator('#password_confirmation').fill('secret-pass');
  await page.locator('form button[type="submit"]').click();
  await expect(page.getByTestId('register-error')).toContainText(
    'already exists',
  );
  await expect(
    page.getByTestId('register-error').getByRole('link', { name: 'Sign in' }),
  ).toBeVisible();

  await page.locator('form button[type="submit"]').click();
  await expect(page.getByTestId('register-error')).toContainText(
    'Too many attempts',
  );

  await page.locator('form button[type="submit"]').click();
  await expect(page.getByTestId('register-error')).toContainText(
    "couldn't reach the server",
  );
  await expect(page.locator('form button[type="submit"]')).toBeEnabled();
  expect(await page.evaluate(() => localStorage.getItem('token'))).toBeNull();
  expect(await markerOf(page)).toBeNull();

  await page.getByTestId('continue-without-account').click();
  await expect(page).toHaveURL(/\/local-/);
  await expect(page.getByTestId('sidebar-create-account')).toBeVisible();

  await page.getByTestId('sidebar-create-account').click();
  await fillRegister(page);
  await expect(upgradeDialog(page)).toBeVisible();
  expect(register()).toBe(4);
});

test('Login offers to continue without an account', async ({ page }) => {
  await localPage(page, { profile });
  await page.goto('/login');
  await page.getByTestId('continue-without-account').click();
  await expect(page).toHaveURL(/\/local-/);
});

test('a failed key migration shows a warning, Retry succeeds and sign-in stays usable', async ({
  page,
}) => {
  const { shell } = await withLocalWorkspace(page, {
    profile,
    loginAs: 'cloud-token',
    upgrade: { failLlm: 1 },
  });
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-/);
  await page.getByRole('button', { name: 'Create account' }).first().click();
  await fillRegister(page);
  await expect(page.getByTestId('local-upgrade-warning')).toBeVisible();
  await expect(page.getByTestId('local-upgrade-key')).toHaveCount(0);
  expect(JSON.parse(await markerOf(page))).toEqual({
    migrate: true,
    dialog: true,
  });
  await page.getByRole('button', { name: 'Retry' }).click();
  await expect(page.getByTestId('local-upgrade-warning')).toHaveCount(0);
  await expect(page.getByTestId('local-upgrade-key')).toContainText('moved');
  expect(JSON.parse(await markerOf(page))).toEqual({
    migrate: false,
    dialog: true,
  });
  expect(
    shell.upgradeCalls.filter((c) => c.command === 'persona_llm_migrate_local'),
  ).toHaveLength(2);
});

test.describe('nothing local is uploaded', () => {
  const bootWrites = [];

  const run = async (page, { linkedTo, upgrade }) => {
    const ctx = await withLocalWorkspace(page, {
      profile: linkedTo ? { ...profile, linked_user_id: linkedTo } : profile,
      loginAs: 'cloud-token',
      upgrade: { keyValue: SENTINEL_KEY, ...upgrade },
    });
    const seen = [];
    page.on('request', (request) => {
      const url = new URL(request.url());
      if (!url.pathname.startsWith('/api/')) return;
      seen.push({
        method: request.method(),
        path: url.pathname,
        text: `${request.url()} ${request.postData() ?? ''}`,
      });
    });
    await page.goto('/');
    await expect(page).toHaveURL(/\/local-/);
    await page.getByRole('button', { name: 'Create account' }).first().click();
    await fillRegister(page);
    await expect(upgradeDialog(page)).toBeVisible();
    if (upgrade.failLlm) {
      await page.getByRole('button', { name: 'Retry' }).click();
      await expect(page.getByTestId('local-upgrade-key')).toBeVisible();
    }
    await page.getByTestId('local-upgrade-stay').click();
    await page.reload();
    await expect(page.getByTestId('sidebar-create-account')).toHaveCount(0);
    return { ...ctx, seen };
  };

  const check = (seen) => {
    expect(seen.length).toBeGreaterThan(0);
    for (const entry of seen) {
      expect(entry.text).not.toContain(SENTINEL_KEY);
      expect(entry.text).not.toContain(SENTINEL_WORKSPACE);
    }
    const writes = seen
      .filter((e) => !['GET', 'HEAD', 'OPTIONS'].includes(e.method))
      .map((e) => `${e.method} ${e.path}`);
    return [...new Set(writes)].sort();
  };

  test('linked account, with a Retry', async ({ page }) => {
    const { seen, shell } = await run(page, { upgrade: { failLlm: 1 } });
    expect(shell.upgradeCalls.length).toBeGreaterThan(3);
    expect(check(seen)).toEqual(
      ['POST /api/auth/register', ...bootWrites].sort(),
    );
  });

  test('account B', async ({ page }) => {
    const { seen, shell } = await run(page, { linkedTo: 99, upgrade: {} });
    expect(shell.upgradeCalls.map((c) => c.command)).toEqual([
      'local_profile_link',
    ]);
    expect(check(seen)).toEqual(
      ['POST /api/auth/register', ...bootWrites].sort(),
    );
  });
});
