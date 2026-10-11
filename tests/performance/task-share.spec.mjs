import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const prepare = async (page, context) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { taskCount: 2 });
};

const clipboard = (page) => page.evaluate(() => navigator.clipboard.readText());

test('task page footer Share copies web, app and Markdown links', async ({
  page,
  context,
}) => {
  await prepare(page, context);
  await page.goto('/demo/tasks/1');
  await page.getByTestId('task-share-trigger').click();

  await page.getByTestId('task-share-copy-link').click();
  expect(await clipboard(page)).toBe('https://tmgr.dev/demo/tasks/1');
  await expect(page.getByText('Link copied').first()).toBeVisible();

  await page.getByTestId('task-share-trigger').click();
  await page.getByTestId('task-share-copy-app-link').click();
  expect(await clipboard(page)).toBe('tmgr://task/demo/1');

  await page.getByTestId('task-share-trigger').click();
  await page.getByTestId('task-share-copy-markdown').click();
  expect(await clipboard(page)).toBe(
    '[Original task](https://tmgr.dev/demo/tasks/1)',
  );

  await page.getByTestId('task-share-trigger').click();
  await expect(page.getByTestId('task-share-open-app')).toBeVisible();
});

test('board card menu has a Share submenu', async ({ page, context }) => {
  await prepare(page, context);
  await page.goto('/demo/board');
  const card = page.locator('[data-task-id="2"]');
  await card.waitFor();
  await card.hover();
  await card.getByRole('button', { name: 'Task actions' }).click();
  await page.getByTestId('task-share-submenu').click();
  await page.getByTestId('task-share-copy-link').click();
  expect(await clipboard(page)).toBe('https://tmgr.dev/demo/tasks/2');
});

test('desktop app does not offer Open in TMGR app', async ({
  page,
  context,
}) => {
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      invoke: () => Promise.resolve(null),
      transformCallback: () => 0,
    };
  });
  await prepare(page, context);
  await page.goto('/demo/tasks/1');
  await page.getByTestId('task-share-trigger').click();
  await expect(page.getByTestId('task-share-copy-link')).toBeVisible();
  await expect(page.getByTestId('task-share-open-app')).toHaveCount(0);
});
