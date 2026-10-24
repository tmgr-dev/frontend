import { expect, test } from '@playwright/test';
import { withData } from './fixtures.mjs';
import { boot } from './visualApp.mjs';

// TM-209: elements that carried two utilities for the same property, where Tailwind 4's rule order
// picked the other class. Expected values are what the Tailwind 3 build (origin/master ec5df95a) renders.

test.beforeEach(async ({ page }, info) => {
  test.skip(
    info.project.name !== 'desktop',
    'computed-style check, one project',
  );
  await boot(page, 'light');
  await withData(page);
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]:visible').first().waitFor();
});

const push = (page, path) =>
  page.evaluate(
    (to) =>
      document
        .querySelector('#app')
        .__vue_app__.config.globalProperties.$router.push(to),
    path,
  );

const opacity = (loc) => loc.evaluate((el) => getComputedStyle(el).opacity);

test('category settings: the code input keeps the 14px text', async ({
  page,
}) => {
  await push(page, '/demo/categories/10');
  const code = page.locator('input[placeholder="e.g., PROJ, FEAT, BUG"]');
  await expect(code).toHaveValue('WORK');
  await expect(code).toHaveCSS('font-size', '14px');
});

test('routines week view: chip opacity for pending, completed and a dragged pending chip', async ({
  page,
}) => {
  await push(page, '/routines');
  await page.getByRole('button', { name: 'Week' }).first().click();
  const chip = (title) =>
    page.locator('.dr-week-chip', { hasText: title }).first();
  const pending = chip('Inbox zero');
  const completed = chip('Stretch');
  await expect(pending).toBeVisible();
  await expect.poll(() => opacity(pending)).toBe('1');
  await expect.poll(() => opacity(completed)).toBe('0.5');

  // A mouse long-press (useRoutineDrag LONG_PRESS_MS = 250) starts the drag without moving.
  await pending.scrollIntoViewIfNeeded();
  const box = await pending.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await expect.poll(() => opacity(pending)).toBe('0.3');
  await page.keyboard.press('Escape');
  await page.mouse.up();
});
