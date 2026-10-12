import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const task = (id, extra = {}) => ({
  id,
  title: `Row task ${id}`,
  description: null,
  description_json: null,
  status: 'created',
  status_id: 1,
  workspace_id: 1,
  user_id: 1,
  user: { id: 1, name: 'Test User' },
  assignees: [],
  settings: [],
  checkpoints: [],
  common_time: 0,
  start_time: 0,
  approximately_time: 0,
  project_category_id: null,
  deleted_at: null,
  created_at: '2026-09-13T10:00:00Z',
  updated_at: '2026-09-13T10:00:00Z',
  ...extra,
});

const openList = async (page) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page);
  const calls = [];
  const nativeDialogs = [];
  page.on('dialog', (dialog) => {
    nativeDialogs.push(dialog.message());
    return dialog.dismiss();
  });
  await page.route('**/api/tasks/current*', (route) =>
    route.fulfill({
      json: {
        data: [task(1), task(2, { deleted_at: '2026-10-01T10:00:00Z' })],
        meta: {
          current_page: 1,
          per_page: 10,
          total: 2,
          last_page: 1,
          from: 1,
          to: 2,
        },
      },
    }),
  );
  await page.route(/\/api\/tasks\/\d+$/, (route) => {
    if (route.request().method() !== 'DELETE') return route.fallback();
    calls.push(`DELETE ${new URL(route.request().url()).pathname}`);
    return route.fulfill({
      json: { data: { deleted_at: '2026-10-12T10:00:00Z' } },
    });
  });
  await page.route(/\/api\/tasks\/\d+\/restore$/, (route) => {
    calls.push(
      `${route.request().method()} ${new URL(route.request().url()).pathname}`,
    );
    return route.fulfill({ json: { data: { deleted_at: null } } });
  });
  await page.goto('/demo/list');
  const live = page.locator('[data-task-id="1"]');
  const trashed = page.locator('[data-task-id="2"]');
  await expect(live).toBeVisible();
  await expect(trashed).toBeVisible();
  return { calls, nativeDialogs, live, trashed };
};

const openMenu = async (page, row) => {
  await row.hover();
  await row.getByRole('button', { name: 'Task actions', exact: true }).click();
  const menu = page.getByRole('menu');
  await expect(menu).toBeVisible();
  return menu;
};

test('the row menu of a live task offers neither Delete nor Restore', async ({
  page,
}) => {
  const { calls, live } = await openList(page);
  const menu = await openMenu(page, live);
  await expect(
    menu.getByRole('menuitem', { name: 'Restore from trash', exact: true }),
  ).toHaveCount(0);
  await expect(
    menu.getByRole('menuitem', { name: 'Delete task', exact: true }),
  ).toHaveCount(0);
  expect(calls).toEqual([]);
});

test('the row menu of a trashed task restores it', async ({ page }) => {
  const { calls, trashed } = await openList(page);
  await expect(trashed).toHaveClass(/opacity-50/);
  const menu = await openMenu(page, trashed);
  await expect(
    menu.getByRole('menuitem', { name: 'Delete task', exact: true }),
  ).toHaveCount(0);

  await menu
    .getByRole('menuitem', { name: 'Restore from trash', exact: true })
    .click();
  await expect.poll(() => calls).toEqual(['POST /api/tasks/2/restore']);
  await expect(trashed).not.toHaveClass(/opacity-50/);

  await page.keyboard.press('Escape');
  const reopened = await openMenu(page, trashed);
  await expect(
    reopened.getByRole('menuitem', { name: 'Restore from trash', exact: true }),
  ).toHaveCount(0);
});

test('list rows have no inline Done, Delete or Restore buttons', async ({
  page,
}) => {
  const { live, trashed } = await openList(page);
  for (const row of [live, trashed]) {
    await row.hover();
    for (const name of [
      'Done',
      'Reactivate',
      'Hide',
      'Delete task',
      'Restore from trash',
      'done',
      'refresh',
      'visibility_off',
      'delete',
      'restore_from_trash',
    ])
      await expect(row.getByRole('button', { name, exact: true })).toHaveCount(
        0,
      );
    await expect(row.locator('.material-icons')).toHaveCount(0);
  }
});
