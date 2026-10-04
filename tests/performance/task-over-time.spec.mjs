import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const openRunningTask = async (page, { settings, approximately }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page);
  const now = Math.floor(Date.now() / 1000);
  await page.route(/\/api\/workspaces\/\d+\/feature-toggles$/, (route) =>
    route.fulfill({
      json: {
        data: Object.fromEntries(
          ['board', 'task.countdown'].map((key) => [
            key,
            { key, name: key, group: 'pages', type: 'boolean', enabled: true },
          ]),
        ),
      },
    }),
  );
  await page.route(/\/api\/tasks\/1$/, (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    return route.fulfill({
      json: {
        data: {
          id: 1,
          title: 'Original task',
          status_id: 1,
          workspace_id: 1,
          user_id: 1,
          assignees: [],
          checkpoints: [],
          settings,
          common_time: 4207,
          start_time: now,
          approximately_time: approximately,
        },
      },
    });
  });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  await expect(page.getByText('Time tracked')).toBeVisible();
};

test('a running timer is not over time when the task has no estimate', async ({
  page,
}) => {
  await openRunningTask(page, { settings: [], approximately: 0 });
  await expect(page.getByText('Running', { exact: true })).toBeVisible();
  await expect(page.getByText('Over time')).toHaveCount(0);
});

test('a running timer uses the estimate from the task settings', async ({
  page,
}) => {
  await openRunningTask(page, {
    settings: [{ id: 1, key: 'approximately_time', value: '28800' }],
    approximately: 0,
  });
  await expect(page.getByText('Running', { exact: true })).toBeVisible();
  await expect(page.getByText('Over time')).toHaveCount(0);
});

test('a running timer past its own estimate is over time', async ({ page }) => {
  await openRunningTask(page, { settings: [], approximately: 3600 });
  await expect(page.getByText('Over time')).toBeVisible();
});
