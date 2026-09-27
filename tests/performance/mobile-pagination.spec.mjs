import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test('on a phone the task list pages by number and fits the screen', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockApp(page);
  await page.goto('/demo/list');
  const pager = page.getByRole('navigation', { name: 'Pagination' });
  await expect(pager).toBeInViewport({ ratio: 1 });
  await expect(pager.getByRole('button', { name: '1', exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(pager.getByRole('button', { name: 'Previous page' })).toBeDisabled();
  await expect(pager.getByText('Showing 1–10 of 20')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Next', exact: true })).toBeHidden();
  expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);

  await pager.getByRole('button', { name: '2', exact: true }).click();
  await expect(page.getByText('Second page task')).toBeVisible();
  await expect(pager.getByRole('button', { name: '2', exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(pager.getByRole('button', { name: 'Next page' })).toBeDisabled();
});

test('on a wide screen the list keeps the classic pagination', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page);
  await page.goto('/demo/list');
  await expect(page.getByRole('button', { name: 'Next', exact: true })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Pagination' })).toBeHidden();
});
