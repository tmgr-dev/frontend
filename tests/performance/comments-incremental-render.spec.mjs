import { expect, test } from '@playwright/test';
import {
  countLifecycle,
  lifecycleOf,
  resetLifecycle,
  survivors,
  trackNodes,
} from './incremental-render.helpers.mjs';
import { mockApp } from './mockApp.mjs';

const comment = (id, message) => ({
  id,
  message,
  user: { id: 1, name: 'Test User' },
  created_at: `2026-09-13T10:00:0${id}Z`,
  updated_at: '2026-09-13T10:00:00Z',
  reactions: [],
});

test('adding a comment keeps the existing comments mounted', async ({
  page,
}) => {
  await countLifecycle(page);
  await mockApp(page);
  const comments = [1, 2, 3].map((id) => comment(id, `Comment ${id}`));
  await page.route('**/api/tasks/1/comments/', (route) =>
    route.fulfill({ json: { data: comments } }),
  );
  await page.route('**/api/tasks/1/comments', async (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    comments.push(comment(4, route.request().postDataJSON().message));
    await route.fulfill({ json: { data: comments[3] } });
  });

  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const rows = '[data-comment-id]';
  await expect(page.locator(rows)).toHaveCount(3);
  await trackNodes(page, rows);
  await resetLifecycle(page);

  const input = page
    .locator('.new-form-container')
    .getByPlaceholder(/comment/i);
  await input.first().click();
  await input.first().fill('Comment 4');
  await input.first().press('Enter');
  await expect(page.locator(rows)).toHaveCount(4);
  await page.waitForTimeout(300);

  expect(await survivors(page, rows)).toEqual({
    kept: 3,
    total: 4,
    removed: 0,
  });
  expect((await lifecycleOf(page, 'MarkdownText')).removed).toBe(0);
});
