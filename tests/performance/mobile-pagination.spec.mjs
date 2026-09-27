import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test('the task list pagination fits a phone screen', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockApp(page);
  await page.goto('/demo/list');
  const next = page.getByRole('button', { name: 'Next' });
  await expect(next).toBeVisible();
  await expect(next).toBeInViewport({ ratio: 1 });
  await expect(page.getByRole('button', { name: 'Previous' })).toBeInViewport({ ratio: 1 });
  await expect(page.getByText('Page 1 of 2')).toBeInViewport({ ratio: 1 });
  const scrollable = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(scrollable).toBe(false);
});
