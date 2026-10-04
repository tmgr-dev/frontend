import { expect, test } from '@playwright/test';
import {
  countLifecycle,
  lifecycleOf,
  resetLifecycle,
  survivors,
  trackNodes,
} from './incremental-render.helpers.mjs';
import { mockApp } from './mockApp.mjs';

test('adding a task to the board leaves the existing cards untouched', async ({
  page,
}) => {
  await countLifecycle(page);
  await mockApp(page, { taskCount: 5 });
  await page.route('**/api/tasks', async (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    const payload = route.request().postDataJSON();
    await route.fulfill({
      json: {
        data: { ...payload, id: 100, updated_at: '2026-09-13T11:00:00Z' },
      },
    });
  });
  await page.goto('/demo/board');
  await expect(page.locator('[data-task-id]')).toHaveCount(5);
  await page.waitForTimeout(500);
  await trackNodes(page, '[data-task-id]');
  await resetLifecycle(page);

  await page.locator('.board-add-task').click();
  await page.getByPlaceholder('Enter task title...').fill('Brand new');
  await page.getByPlaceholder('Enter task title...').press('Enter');
  await expect(page.locator('[data-task-id="100"]')).toHaveCount(1);
  await page.waitForTimeout(300);

  expect(await survivors(page, '[data-task-id]')).toEqual({
    kept: 5,
    total: 6,
    removed: 0,
  });
  expect(await lifecycleOf(page, 'ViewportTaskCard')).toEqual({
    added: 1,
    updated: 0,
    removed: 0,
  });
});

test('updating one task re-renders only its own card', async ({ page }) => {
  await countLifecycle(page);
  await mockApp(page, { taskCount: 5 });
  await page.goto('/demo/board');
  await expect(page.locator('[data-task-id]')).toHaveCount(5);
  await page.waitForTimeout(500);
  await trackNodes(page, '[data-task-id]');
  await resetLifecycle(page);

  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit('updateSingleTask', {
        id: 2,
        title: 'Changed title',
        status_id: 1,
      }),
  );
  await expect(page.getByText('Changed title')).toBeVisible();
  await page.waitForTimeout(300);

  expect(await survivors(page, '[data-task-id]')).toEqual({
    kept: 5,
    total: 5,
    removed: 0,
  });
  expect((await lifecycleOf(page, 'ViewportTaskCard')).updated).toBe(1);
});
