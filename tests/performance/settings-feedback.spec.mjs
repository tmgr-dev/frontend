import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const TOKEN_FIXTURE = 'fixture-token-not-a-secret';

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
  });
});

const toasts = (page) =>
  page.getByRole('region', { name: /^Notifications/ }).getByRole('status');

async function mockSettings(page, { saveStatus = 200 } = {}) {
  await mockApp(page, { userExtra: { has_smart_device_token: false } });
  await page.route('**/api/workspaces/*/modules', (route) =>
    route.fulfill({
      json: {
        data: {
          configured: true,
          modules: [{ key: 'mcp', enabled: true }],
        },
      },
    }),
  );
  const state = { saveStatus, generate: 200, revoke: 200, hasToken: false };
  await page.route('**/api/v2/user/settings', (route) => {
    if (route.request().method() === 'PUT' && state.saveStatus !== 200)
      return route.fulfill({
        status: state.saveStatus,
        json: { message: 'Save rejected' },
      });
    return route.fulfill({ json: { data: [] } });
  });
  await page.route('**/api/user/settings', (route) =>
    route.fulfill({ json: { data: {} } }),
  );
  await page.route('**/api/smart-device/token/generate', (route) => {
    if (state.generate !== 200)
      return route.fulfill({
        status: state.generate,
        json: { message: 'Token service down' },
      });
    state.hasToken = true;
    return route.fulfill({ json: { data: { token: TOKEN_FIXTURE } } });
  });
  await page.route('**/api/smart-device/token/revoke', (route) => {
    if (state.revoke !== 200)
      return route.fulfill({
        status: state.revoke,
        json: { message: 'Revoke refused' },
      });
    state.hasToken = false;
    return route.fulfill({ json: { data: null } });
  });
  return state;
}

test.describe('notification settings save', () => {
  test('success shows a toast', async ({ page }) => {
    await mockSettings(page);
    await page.goto('/settings?tab=notification');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(
      toasts(page).filter({ hasText: 'Settings updated' }),
    ).toHaveCount(1);
  });

  test('failure shows a destructive toast', async ({ page }) => {
    await mockSettings(page, { saveStatus: 500 });
    await page.goto('/settings?tab=notification');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    const toast = toasts(page).filter({ hasText: 'Failed to update settings' });
    await expect(toast).toHaveCount(1);
    await expect(toast).toHaveClass(/destructive/);
  });
});

test.describe('smart device token', () => {
  test('generation success shows a toast and no token value', async ({
    page,
  }) => {
    const logged = [];
    page.on('console', (msg) => logged.push(msg.text()));
    await mockSettings(page);
    await page.goto('/settings?tab=device');
    await page.getByRole('button', { name: 'Generate token' }).click();
    await page.getByRole('button', { name: 'Ok', exact: true }).click();
    const toast = toasts(page).filter({ hasText: 'Token generated' });
    await expect(toast).toHaveCount(1);
    await expect(toast).not.toContainText(TOKEN_FIXTURE);
    expect(logged.join('\n')).not.toContain(TOKEN_FIXTURE);
  });

  test('generation success is not reported as a failure when the user refresh fails', async ({
    page,
  }) => {
    const state = await mockSettings(page);
    await page.route('**/api/user', (route) =>
      state.hasToken
        ? route.fulfill({ status: 500, json: { message: 'User down' } })
        : route.fallback(),
    );
    await page.goto('/settings?tab=device');
    await page.getByRole('button', { name: 'Generate token' }).click();
    await page.getByRole('button', { name: 'Ok', exact: true }).click();
    await expect(
      toasts(page).filter({ hasText: 'Token generated' }),
    ).toHaveCount(1);
    await expect(toasts(page).filter({ hasText: 'User down' })).toHaveCount(0);
    await expect(
      toasts(page).filter({ hasText: 'Failed to generate token' }),
    ).toHaveCount(0);
    await expect(
      page.getByRole('button', { name: 'Ok', exact: true }),
    ).toHaveCount(0);
  });

  test('generation failure closes the dialog and shows an error toast', async ({
    page,
  }) => {
    const state = await mockSettings(page);
    state.generate = 500;
    await page.goto('/settings?tab=device');
    await page.getByRole('button', { name: 'Generate token' }).click();
    await expect(
      page.getByText('Generate a new token for your smart devices?'),
    ).toBeVisible();
    await page.getByRole('button', { name: 'Ok', exact: true }).click();
    const toast = toasts(page).filter({ hasText: 'Token service down' });
    await expect(toast).toHaveCount(1);
    await expect(toast).toHaveClass(/destructive/);
    await expect(
      page.getByText('Generate a new token for your smart devices?'),
    ).toHaveCount(0);
    await expect(
      page.getByRole('button', { name: 'Ok', exact: true }),
    ).toHaveCount(0);
  });

  async function revokeSetup(page, revoke) {
    const state = await mockSettings(page);
    state.revoke = revoke;
    await page.route('**/api/user', (route) =>
      route.fulfill({
        json: {
          data: {
            id: 1,
            name: 'Test User',
            email: 'test@example.test',
            settings: [{ key: 'current_workspace', value: '1' }],
            created_at: '2026-01-01T10:00:00Z',
            has_smart_device_token: true,
          },
        },
      }),
    );
    await page.goto('/settings?tab=device');
    await page.getByRole('button', { name: 'Revoke token' }).click();
    await page.getByRole('button', { name: 'Ok', exact: true }).click();
  }

  test('revoke success shows a toast', async ({ page }) => {
    await revokeSetup(page, 200);
    await expect(toasts(page).filter({ hasText: 'Token revoked' })).toHaveCount(
      1,
    );
  });

  test('revoke failure closes the dialog and shows an error toast', async ({
    page,
  }) => {
    await revokeSetup(page, 500);
    const toast = toasts(page).filter({ hasText: 'Revoke refused' });
    await expect(toast).toHaveCount(1);
    await expect(toast).toHaveClass(/destructive/);
    await expect(
      page.getByRole('button', { name: 'Ok', exact: true }),
    ).toHaveCount(0);
  });
});

const pageRow = (id, title) => ({
  id,
  title,
  slug: `page-${id}`,
  type: 'plain',
  parent_id: null,
  position: id,
  pinned: false,
  updated_at: '2026-10-01T10:00:00Z',
});

test.describe('pages index retry', () => {
  async function mockPages(page) {
    await mockApp(page);
    const state = { failing: true, treeRequests: 0 };
    await page.route('**/api/pages**', (route) => {
      const path = new URL(route.request().url()).pathname;
      if (/\/pages(\/tree)?$/.test(path)) {
        state.treeRequests += 1;
        if (state.failing)
          return route.fulfill({ status: 500, json: { message: 'down' } });
        return route.fulfill({
          json: { data: [pageRow(1, 'Handbook'), pageRow(2, 'Roadmap')] },
        });
      }
      return route.fulfill({ json: { data: [] } });
    });
    return state;
  }

  test('Retry reloads the list after a failed load', async ({ page }) => {
    const state = await mockPages(page);
    await page.goto('/demo/pages');
    const index = page.getByTestId('pages-index');
    await expect(index.getByText('Failed to load pages')).toBeVisible();
    state.failing = false;
    await index.getByRole('button', { name: 'Retry' }).click();
    await expect(index.getByText('Failed to load pages')).toHaveCount(0);
    await expect(index.getByTestId('pages-index-tree')).toContainText(
      'Handbook',
    );
  });

  test('a failed retry keeps the error and Retry', async ({ page }) => {
    const state = await mockPages(page);
    await page.goto('/demo/pages');
    const index = page.getByTestId('pages-index');
    const retry = index.getByRole('button', { name: 'Retry' });
    await expect(retry).toBeVisible();
    const before = state.treeRequests;
    await retry.click();
    await expect.poll(() => state.treeRequests).toBeGreaterThan(before);
    await expect(index.getByText('Failed to load pages')).toBeVisible();
    await expect(retry).toBeVisible();
  });
});

test('a notification row is exposed as a link', async ({ page }) => {
  await mockApp(page);
  await page.route('**/api/notifications**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/unread-count'))
      return route.fulfill({ json: { count: 1 } });
    return route.fulfill({
      json: {
        data: [
          {
            id: 1,
            user_id: 1,
            workspace_id: 1,
            activity_id: null,
            type: 'task_updated',
            title: 'Notification 1',
            message: 'Message 1',
            data: null,
            link: null,
            read_at: null,
            created_at: '2026-10-01T10:00:00Z',
            updated_at: '2026-10-01T10:00:00Z',
          },
        ],
        current_page: 1,
        last_page: 1,
        per_page: 20,
        total: 1,
      },
    });
  });
  await page.goto('/demo/notifications');
  await expect(
    page.getByRole('main').getByRole('link', { name: /Notification 1/ }),
  ).toBeVisible();
});

for (const [name, act] of [
  ['click', (row) => row.click()],
  ['Enter', (row) => row.focus().then(() => row.press('Enter'))],
  ['Space', (row) => row.focus().then(() => row.press('Space'))],
]) {
  test(`a notification link row navigates on ${name}`, async ({ page }) => {
    await mockApp(page);
    const reads = [];
    await page.route('**/api/notifications**', (route) => {
      const req = route.request();
      const path = new URL(req.url()).pathname;
      if (path.endsWith('/unread-count'))
        return route.fulfill({ json: { count: 1 } });
      if (req.method() === 'POST' && path.endsWith('/read')) {
        reads.push(path);
        return route.fulfill({ json: { data: {} } });
      }
      return route.fulfill({
        json: {
          data: [
            {
              id: 7,
              user_id: 1,
              workspace_id: 1,
              activity_id: null,
              type: 'task_updated',
              title: 'Go somewhere',
              message: 'Message 7',
              data: null,
              link: '/settings?tab=notification',
              read_at: null,
              created_at: '2026-10-01T10:00:00Z',
              updated_at: '2026-10-01T10:00:00Z',
            },
          ],
          current_page: 1,
          last_page: 1,
          per_page: 20,
          total: 1,
        },
      });
    });
    await page.goto('/demo/notifications');
    const row = page
      .getByRole('main')
      .getByRole('link', { name: /Go somewhere/ });
    await act(row);
    await expect(page).toHaveURL(/\/settings\?tab=notification$/);
    expect(reads).toHaveLength(1);
  });
}

test('Invite and Delete workspace are named buttons below lg', async ({
  page,
}) => {
  await page.setViewportSize({ width: 800, height: 900 });
  await mockApp(page);
  await page.route(/\/api\/workspaces(\?.*)?$/, (route) =>
    route.fulfill({
      json: {
        data: [
          {
            id: 1,
            name: 'Demo',
            code: 'demo',
            user_id: 1,
            type: 'team',
            is_default: false,
          },
        ],
      },
    }),
  );
  await page.goto('/settings/workspaces');
  const invite = page.getByRole('button', { name: 'Invite', exact: true });
  const remove = page.getByRole('button', {
    name: 'Delete workspace',
    exact: true,
  });
  await expect(invite).toBeVisible();
  await expect(remove).toBeVisible();
  await expect(invite.locator('span')).toBeHidden();
  await expect(remove.locator('span')).toBeHidden();

  await remove.click();
  const dialog = page.getByRole('alertdialog', { name: 'Delete workspace' });
  const confirm = dialog.getByRole('button', { name: 'Delete workspace' });
  await expect(confirm).toBeDisabled();
  await dialog.getByLabel('Workspace name').fill('Other');
  await expect(confirm).toBeDisabled();
  await dialog.getByLabel('Workspace name').fill('Demo');
  await expect(confirm).toBeEnabled();
});
