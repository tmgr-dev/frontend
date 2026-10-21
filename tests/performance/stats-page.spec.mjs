import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const STATS = {
  users: 91234,
  active_in_1_day: 120,
  active_in_1_week: 456,
  active_in_1_month: 789,
  active_in_1_year: 1234,
  tasks: 55555,
  hours: 91234.5,
};

test('renders formatted stat tiles and the week value under Last 7 days', async ({
  page,
}) => {
  await mockApp(page);
  await page.route('**/api/stats', (route) =>
    route.fulfill({ json: { data: STATS } }),
  );
  await page.goto('/stats');

  await expect(page.getByText('91,234').first()).toBeVisible();
  await expect(page.getByText('55,555').first()).toBeVisible();

  const weekLabel = page.getByText('Last 7 days', { exact: true });
  await expect(weekLabel.locator('xpath=following-sibling::span')).toHaveText(
    '456',
  );
});

test('shows no NaN when the stats payload is empty', async ({ page }) => {
  await mockApp(page);
  await page.route('**/api/stats', (route) =>
    route.fulfill({ json: { data: {} } }),
  );
  await page.goto('/stats');

  await expect(page.getByText('—').first()).toBeVisible();
  await expect(page.getByText('NaN')).toHaveCount(0);
});

test('the sidebar Tools group links to Statistics', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await mockApp(page, { onboarding: {} });
  await page.goto('/demo/board');

  await page
    .locator('[data-sidebar="content"]')
    .getByRole('link', { name: 'Statistics' })
    .click();
  await expect(page).toHaveURL(/\/stats$/);
});
