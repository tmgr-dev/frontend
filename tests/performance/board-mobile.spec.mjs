import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

test.use({
  viewport: { width: 390, height: 664 },
  hasTouch: true,
  isMobile: true,
});

test('a long column on a phone ends inside the screen and scrolls', async ({
  page,
}) => {
  await mockApp(page, { taskCount: 30 });
  await page.goto('/demo/board');
  await page.getByText('Fixture task 2').first().waitFor();

  const list = page.locator('.board-card-draggable').first();
  const measure = () =>
    list.evaluate((el) => ({
      overflow: el.getBoundingClientRect().bottom - window.innerHeight,
      scrollable: el.scrollHeight > el.clientHeight,
    }));
  await expect
    .poll(async () => (await measure()).overflow)
    .toBeLessThanOrEqual(0);
  expect((await measure()).scrollable).toBe(true);

  await list.evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });
  const last = page.getByText('Fixture task 30').first();
  await expect(last).toBeInViewport();
});

test('the column add button shows without hover on touch screens', async ({
  page,
}) => {
  await mockApp(page, { taskCount: 3 });
  await page.goto('/demo/board');
  await page.getByText('Fixture task 2').first().waitFor();

  expect(await page.evaluate(() => matchMedia('(hover: none)').matches)).toBe(
    true,
  );
  await expect(
    page.locator('[data-tour="board.add-in-column"]').first(),
  ).toHaveCSS('opacity', '1');
});

test('an unassigned card offers the assign button on touch screens', async ({
  page,
}) => {
  await mockApp(page, { taskCount: 3 });
  await page.goto('/demo/board');
  await page.getByText('Fixture task 2').first().waitFor();

  await expect(page.getByTitle('Assign someone').first()).toBeVisible();
});
