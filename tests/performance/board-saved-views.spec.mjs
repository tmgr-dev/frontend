import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

test('a personal board view survives reload and edits without adding history entries', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page);
  const settings = {
    showTooltips: false,
    'boardView.2.other': { name: 'Another workspace', query: {} },
  };
  let rejectSave = true;
  await page.route('**/api/user/settings', async (route) => {
    if (route.request().method() === 'PATCH') {
      if (rejectSave) {
        rejectSave = false;
        return route.fulfill({ status: 503, json: { message: 'Unavailable' } });
      }
      Object.assign(settings, route.request().postDataJSON());
    }
    return route.fulfill({ json: { data: { settings } } });
  });
  await page.goto('/demo/list');
  await page.goto('/demo/board?keep=yes');
  const myWork = page.getByRole('button', { name: 'My work' });
  await myWork.click();
  await expect(page).toHaveURL(/user=1/);
  const openFilters = () =>
    page.getByRole('button', { name: /^Filters/ }).click();
  const savedViews = page
    .locator('details')
    .filter({ has: page.getByText('Saved views', { exact: true }) });
  await openFilters();
  await page.getByRole('button', { name: 'High', exact: true }).click();
  await savedViews.locator('summary').click();
  await savedViews.getByLabel('View name').fill('Mine');
  await savedViews.getByRole('button', { name: 'Save view' }).click();
  await expect(savedViews.getByRole('alert')).toBeVisible();
  await savedViews.getByRole('button', { name: 'Save view' }).click();
  await expect(
    savedViews.getByRole('button', { name: 'Mine', exact: true }),
  ).toBeVisible();
  const key = Object.keys(settings).find((k) => k.startsWith('boardView.1.'));
  expect(settings[key]).toEqual({
    name: 'Mine',
    query: { user: '1', priority: 'high' },
  });
  expect(settings.showTooltips).toBe(false);
  await page.keyboard.press('Escape');
  await page.getByTitle('Clear filters').click();
  await expect(page).not.toHaveURL(/user=/);
  await page.reload();
  await openFilters();
  await savedViews.locator('summary').click();
  await expect(savedViews.getByText('Another workspace')).toHaveCount(0);
  await savedViews.getByRole('button', { name: 'Rename' }).click();
  await savedViews.getByLabel('View name').fill('Assigned');
  await savedViews.getByRole('button', { name: 'Save', exact: true }).click();
  await savedViews
    .getByRole('button', { name: 'Assigned', exact: true })
    .click();
  await expect(page).toHaveURL(/user=1/);
  await expect(page).toHaveURL(/priority=high/);
  await expect(page).toHaveURL(/keep=yes/);
  await savedViews.getByRole('button', { name: 'Delete' }).click();
  await expect(
    savedViews.getByRole('button', { name: 'Assigned', exact: true }),
  ).toHaveCount(0);
  await page.goBack();
  await expect(page).toHaveURL(/\/demo\/list$/);
});

test('local views survive reload on a narrow screen and stay on the device', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 740 });
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:onboarding:demo-offered', 'declined');
    localStorage.setItem(
      'tmgr:onboarding:v1',
      JSON.stringify({ intent: 'skipped', tips_mode: 'off' }),
    );
  });
  const requests = [];
  await desktopPage(
    page,
    { account: false, apiRequests: requests },
    {
      profile: {
        id: -1,
        uuid: '00000000-0000-4000-8000-000000000001',
        name: 'Ada',
        created_at: '2026-10-11T10:00:00Z',
      },
    },
  );
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await page.goto('/local-my-workspace/board?search=private');
  await page.getByTitle('Filters', { exact: true }).click();
  await page.getByText('Saved views', { exact: true }).click();
  await page.getByLabel('View name').fill('Local view');
  await page.getByRole('button', { name: 'Save view' }).click();
  await expect(page.getByRole('button', { name: 'Local view' })).toBeVisible();
  await page.goto('/local-my-workspace/board');
  await page.getByTitle('Filters', { exact: true }).click();
  await page.getByText('Saved views', { exact: true }).click();
  await page.getByRole('button', { name: 'Local view' }).click();
  await expect(page).toHaveURL(/search=private/);
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'close', exact: true })
    .click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page).toHaveURL(/search=private/);
  await page.reload();
  await expect(page).toHaveURL(/search=private/);
  expect(
    requests.filter((request) => request.includes('user/settings')),
  ).toEqual([]);
});

test('switching from a local board to a cloud board drops the local view filters', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  const shell = await desktopPage(page, { account: true });
  await shell('local_workspace_create', { name: 'Private workspace' });
  const cloudRequests = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname.startsWith('/api/'))
      cloudRequests.push(request.url() + (request.postData() ?? ''));
  });
  await page.goto(
    '/local-private-workspace/board?search=device-only-example&priority=high&keep=yes',
  );
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /Demo/ }).click();
  await expect(page).toHaveURL(/\/demo\/board\?keep=yes$/);
  await expect(page.locator('[data-task-id]').first()).toBeVisible();
  expect(
    cloudRequests.filter((request) => request.includes('device-only-example')),
  ).toEqual([]);
});

test('My work rechecks membership when a task is reassigned', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await mockApp(page);
  let assigneeId = 1;
  const row = {
    id: 91,
    title: 'Triage the release',
    status: 'created',
    user_id: 1,
    user: { id: 1, name: 'Test User' },
    settings: [],
    checkpoints: [],
    common_time: 0,
    created_at: '2026-01-01T10:00:00Z',
    updated_at: '2026-01-01T10:00:00Z',
    status_id: 1,
    workspace_id: 1,
    priority: 'high',
  };
  await page.route('**/api/tasks/status/1?**', (route) =>
    route.fulfill({
      json: {
        data: [{ ...row, assignees: [{ id: assigneeId, name: 'Member' }] }],
      },
    }),
  );
  await page.goto('/demo/board?user=1&priority=high');
  const card = page.locator('[data-task-id="91"]').first();
  await expect(card).toBeVisible();
  assigneeId = 2;
  await page.evaluate(
    (task) =>
      document
        .querySelector('#app')
        .__vue_app__.config.globalProperties.$store.commit(
          'updateSingleTask',
          task,
        ),
    { ...row, assignees: [{ id: 2 }] },
  );
  await expect(card).toHaveCount(0);
  assigneeId = 1;
  await page.evaluate(
    (task) =>
      document
        .querySelector('#app')
        .__vue_app__.config.globalProperties.$store.commit(
          'updateSingleTask',
          task,
        ),
    { id: 91, status_id: 1 },
  );
  await expect(card).toBeVisible();
});

test('a bookmark to a missing local board cannot send the search to the active cloud workspace', async ({
  page,
}) => {
  await desktopPage(page, { account: true });
  await page.clock.install();
  const requests = [];
  page.on('request', (req) => {
    if (new URL(req.url()).pathname.startsWith('/api/'))
      requests.push(req.url() + (req.postData() ?? ''));
  });
  await page.goto(
    '/local-deleted/board?search=deleted-private-keyword&keep=yes',
  );
  await expect(
    page.getByRole('alert').filter({ hasText: 'Could not load the board' }),
  ).toBeVisible();
  await expect(page.locator('[data-task-id]')).toHaveCount(0);
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit('taskCreated', {
        id: 91,
        status_id: 1,
      }),
  );
  await page.clock.runFor(500);
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /Demo/ }).click();
  await expect(page).toHaveURL(/\/demo\/board\?keep=yes$/);
  await expect(page.locator('[data-task-id]').first()).toBeVisible();
  expect(requests.filter((r) => r.includes('deleted-private-keyword'))).toEqual(
    [],
  );
});
