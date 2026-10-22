import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const task = (id, statusId) => ({
  id,
  title: `Status task ${id}`,
  description: null,
  description_json: null,
  status: 'created',
  status_id: statusId,
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
});

const meta = (total) => ({
  current_page: 1,
  per_page: 10,
  total,
  last_page: 1,
  from: total ? 1 : 0,
  to: total,
});

const currentTasks = {
  data: [
    { ...task(1, 1), common_time: 3600 },
    task(2, 2),
    { ...task(3, 1), common_time: 1800 },
  ],
  meta: meta(3),
};

const openList = async (
  page,
  { workflow = true, countdown = false, current = currentTasks } = {},
) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page, { multiStatus: true });
  await page.route('**/api/workspaces/*/modules', (route) =>
    route.fulfill({
      json: {
        data: {
          modules: [
            { key: 'board', enabled: true },
            { key: 'workflow', enabled: workflow },
            { key: 'categories', enabled: true },
            ...(countdown ? [{ key: 'task.countdown', enabled: true }] : []),
          ],
        },
      },
    }),
  );
  await page.route('**/api/tasks/current*', (route) =>
    route.fulfill({ json: current }),
  );
  await page.goto('/demo/list');
  await expect(page.locator('[data-task-id="1"]')).toBeVisible();
};

test('filters the list by one or more statuses and clears', async ({
  page,
}, testInfo) => {
  await openList(page);
  await expect(page.locator('body')).toContainText(
    '3 tasks · 1 hour 30 minutes',
  );
  const filters = page.getByTitle('filters');
  await filters.click();
  await expect(page.getByRole('group', { name: 'Status' })).toBeVisible();
  await page.getByRole('button', { name: 'In progress', exact: true }).click();
  await page.keyboard.press('Escape');
  await expect(page.locator('[data-task-id="2"]')).toBeVisible();
  await expect(page.locator('[data-task-id="1"]')).toHaveCount(0);
  await expect(page.locator('[data-task-id="3"]')).toHaveCount(0);
  await expect(page.locator('body')).toContainText('1 task · 0 minutes');
  await page.waitForTimeout(500);
  await page.screenshot({
    path: testInfo.outputPath('status-filter-light.png'),
  });

  await filters.click();
  await page.getByRole('button', { name: 'Backlog', exact: true }).click();
  await page.keyboard.press('Escape');
  await expect(page.locator('[data-task-id="1"]')).toBeVisible();
  await expect(page.locator('[data-task-id="2"]')).toBeVisible();

  await filters.click();
  await page.getByRole('button', { name: 'reset' }).click();
  await page.keyboard.press('Escape');
  await expect(page.locator('[data-task-id="3"]')).toBeVisible();
});

test('shows a readable empty state and clears the status filter', async ({
  page,
}, testInfo) => {
  await openList(page);
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.route('**/api/tasks/current*', (route) =>
    route.fulfill({ json: { data: [task(1, 1)], meta: meta(1) } }),
  );
  await page.getByTitle('filters').click();
  await page.getByRole('button', { name: 'In progress', exact: true }).click();
  await page.waitForTimeout(500);
  await page.screenshot({
    path: testInfo.outputPath('status-filter-dialog-dark.png'),
  });
  await page.keyboard.press('Escape');
  await expect(page.getByText('No tasks match your filters')).toBeVisible();
  await expect(page.locator('body')).toContainText('0 tasks · 0 minutes');
  await page.getByRole('button', { name: 'Clear filters' }).click();
  await expect(page.locator('[data-task-id="1"]')).toBeVisible();
});

test('hides the status filter in simple mode', async ({ page }) => {
  await openList(page, { workflow: false });
  await expect(page.getByRole('group', { name: 'Show tasks' })).toBeVisible();
  await page.getByTitle('filters').click();
  await expect(page.getByRole('heading', { name: 'Filters' })).toBeVisible();
  await expect(page.getByRole('group', { name: 'Status' })).toHaveCount(0);
});

test('the header total follows a timer stopped on a row', async ({ page }) => {
  await openList(page, {
    countdown: true,
    current: {
      data: [
        { ...task(1, 1), common_time: 3600 },
        { ...task(2, 2), start_time: 1 },
      ],
      meta: { ...meta(2), total: 12, total_seconds: '36000' },
    },
  });
  await page.route('**/api/tasks/2/countdown', (route) =>
    route.fulfill({
      json: { data: { ...task(2, 2), common_time: 600, start_time: 0 } },
    }),
  );
  await expect(page.locator('body')).toContainText(
    '12 tasks · 10 hours 0 minutes',
  );
  await page.locator('[data-task-id="2"] button.bg-status-fix').click();
  await expect(page.locator('body')).toContainText(
    '12 tasks · 10 hours 10 minutes',
  );
});

test('a return visit shows the cached total, not total plus page', async ({
  page,
}) => {
  await openList(page, {
    current: {
      data: [{ ...task(1, 1), common_time: 3600 }],
      meta: { ...meta(1), total: 12, total_seconds: '36000' },
    },
  });
  await expect(page.locator('body')).toContainText(
    '12 tasks · 10 hours 0 minutes',
  );
  await page.getByRole('link', { name: 'Board' }).first().click();
  await expect(page).not.toHaveURL(/\/demo\/list/);
  await page.route('**/api/tasks/current*', () => {});
  await page.goBack();
  await expect(page).toHaveURL(/\/demo\/list/);
  await expect(page.locator('[data-task-id="1"]')).toBeVisible();
  await expect(page.locator('body')).toContainText(
    '12 tasks · 10 hours 0 minutes',
  );
});
