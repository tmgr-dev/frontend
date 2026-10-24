import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

// First-visit tours (board) auto-offer ~600ms after landing and toggle the sheet.
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
});

test('the sidebar slides in on a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockApp(page);
  await page.goto('/demo/board');
  // A sheet opened before boot settles can close again; let the app settle.
  await page.waitForLoadState('networkidle');
  await page.locator('[data-sidebar="trigger"]').first().click();
  const sheet = page.locator('[data-mobile="true"][data-sidebar="sidebar"]');
  await expect(sheet).toBeVisible();
  await expect(sheet).toHaveCSS('position', 'fixed');
  await expect
    .poll(() => sheet.evaluate((el) => el.getAnimations().length))
    .toBe(0);
  await expect(sheet).toHaveCSS('transform', 'none');
  const box = await sheet.boundingBox();
  expect(box.x).toBe(0);
  expect(box.y).toBe(0);
  expect(box.height).toBe(844);
  await expect(sheet.getByTitle('Switch workspace').first()).toBeInViewport();
});

test('the desktop sidebar keeps its dotted background layer', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page);
  await page.goto('/demo/board');
  const sidebar = page.locator('[data-sidebar="sidebar"]').first();
  await expect(sidebar).toHaveCSS('position', 'relative');
});
