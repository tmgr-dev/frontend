import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

test('horizontal wheel over a card scrolls the board in the desktop app', async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      invoke: () => Promise.resolve(null),
      transformCallback: () => 0,
    };
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { taskCount: 12 });
  const statuses = Array.from({ length: 8 }, (_, i) => ({
    id: i + 1,
    name: `Column ${i + 1}`,
    type: i ? 'active' : 'default',
    color: '#888888',
    pivot: { order: i + 1, is_active: true },
  }));
  await page.route(/\/api\/(workspaces\/)?statuses$/, (route) =>
    route.fulfill({ json: { data: statuses } }),
  );

  await page.goto('/demo/board');
  const card = page.getByText('Fixture task 2').first();
  await card.waitFor();
  const board = page.locator('.board-container .board-container').first();
  expect(
    await board.evaluate((el) => el.scrollWidth > el.clientWidth),
  ).toBe(true);

  await card.hover();
  await page.mouse.wheel(400, 0);

  await expect
    .poll(() => board.evaluate((el) => el.scrollLeft))
    .toBeGreaterThan(0);
});
