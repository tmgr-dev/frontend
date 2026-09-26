import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

test('the built-in estimate plugin draws its badge, status bar item, page and task section', async ({
  page,
}) => {
  const shell = await desktopPage(page);
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);

  const [inProgress] = await shell('local_db_select', {
    code: 'personal',
    sql: `SELECT id FROM statuses WHERE type = 'active'`,
    params: [],
  });
  await shell('local_db_execute', {
    code: 'personal',
    sql: `INSERT INTO tasks (title, status_id, common_time, approximately_time, created_at, updated_at)
          VALUES ('Over budget', ?, 10800, 7200, '2026-09-26T10:00:00Z', '2026-09-26T10:00:00Z')`,
    params: [inProgress.id],
  });
  await page.goto('/local-personal/board');

  await expect(page.getByText('150%').first()).toBeVisible();
  await expect(
    page.locator('footer').getByText('1h 0m over estimate'),
  ).toBeVisible();

  await page.getByRole('link', { name: 'Estimate vs actual' }).click();
  await expect(page).toHaveURL(
    /\/local-personal\/plugins\/tmgr\.estimate\/report$/,
  );
  await expect(
    page.getByRole('heading', { name: 'Estimate vs actual' }),
  ).toBeVisible();
  await expect(page.getByRole('cell', { name: /Over budget/ })).toBeVisible();

  await page.getByRole('button', { name: /Over budget/ }).click();
  await expect(page.getByText('Spent').first()).toBeVisible();
  await expect(page.getByText('3h 0m').first()).toBeVisible();
  await page.keyboard.press('Escape');

  await page.goto('/settings/plugins');
  const card = page.locator('article', { hasText: 'Estimate vs actual' });
  await expect(card.getByText('Running')).toBeVisible();
  await expect(
    card.getByText('read tasks, read statuses, read tracked time'),
  ).toBeVisible();
  await card.getByRole('switch').click();
  await expect(card.getByText('Off')).toBeVisible();
  await expect(page.locator('footer').getByText('over estimate')).toHaveCount(
    0,
  );
});
