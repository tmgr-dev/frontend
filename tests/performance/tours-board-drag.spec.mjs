import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';
import { AUTO } from './toursHelpers.mjs';

// Tours are NOT suppressed here: the board first-visit tour must wait for the drag.
test('the board first-visit tour waits for a card drag and shows after the drop', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { multiStatus: true, onboarding: AUTO });
  await page.clock.install();
  const statusSaved = page.waitForRequest(
    (request) =>
      request.method() === 'PUT' && request.url().endsWith('/api/tasks/1/2'),
  );
  await page.goto('/demo/board');

  const handle = page.locator(
    '.board-card-draggable[data-status="1"] [data-task-id="1"] .task-drag-handle',
  );
  await handle.waitFor();
  const box = await handle.boundingBox();
  expect(
    await page.evaluate(() => sessionStorage.getItem('tmgr:tours:auto')),
    'the drag must start before the tour is offered',
  ).toBeNull();
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 15, box.y + 15, { steps: 5 });
  await expect(page.locator('.sortable-fallback')).toBeVisible();

  const tour = page.locator('[role="dialog"][aria-labelledby^="tour-"]');
  // Hold the card past the 600 ms route settle (ROUTE_SETTLE_MS) twice over.
  await page.clock.runFor(1500);
  await expect(page.locator('.sortable-fallback')).toBeVisible();
  await expect(tour).toHaveCount(0);

  const target = await page
    .locator('.board-card-draggable[data-status="2"]')
    .boundingBox();
  await page.mouse.move(target.x + target.width / 2, target.y + 35, {
    steps: 20,
  });
  await page.mouse.up();
  await statusSaved;
  await expect(
    page.locator('.board-card-draggable[data-status="2"] [data-task-id="1"]'),
  ).toBeVisible();
  await expect(tour.first()).toContainText('Columns');
});
