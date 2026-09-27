import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test('the sidebar slides in on a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockApp(page);
  await page.goto('/demo/board');
  await page.locator('[data-sidebar="trigger"]').first().click();
  const sheet = page.locator('[data-mobile="true"][data-sidebar="sidebar"]');
  await expect(sheet).toBeVisible();
  await expect(sheet).toHaveCSS('position', 'fixed');
  await expect(sheet).toHaveCSS('transform', 'none');
  const box = await sheet.boundingBox();
  expect(box.x).toBe(0);
  expect(box.y).toBe(0);
  expect(box.height).toBe(844);
  await expect(sheet.getByTitle('Switch workspace').first()).toBeInViewport();
});

test('the desktop sidebar keeps its dotted background layer', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page);
  await page.goto('/demo/board');
  const sidebar = page.locator('[data-sidebar="sidebar"]').first();
  await expect(sidebar).toHaveCSS('position', 'relative');
});
