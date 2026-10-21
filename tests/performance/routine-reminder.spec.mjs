import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const openLocalRoutines = async (page) => {
  const shell = await desktopPage(page);
  // The desktop.move-to-cloud tour offer fires once a local workspace exists
  // and navigates to Settings → Local data mid-test.
  await page.addInitScript(() =>
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1'),
  );
  // routines.first auto-offers 600ms after landing on /routines and covers the page.
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  await page.goto('/routines');
  return shell;
};

const reminderOf = async (shell) =>
  (
    await shell('local_db_select', {
      code: 'personal',
      sql: 'SELECT reminder_min FROM routine_patterns',
      params: [],
    })
  )[0]?.reminder_min;

test('a repeating routine reminder is saved and shown again on Edit', async ({
  page,
}) => {
  const shell = await openLocalRoutines(page);
  const reminder = page.locator('[data-tour="routine.reminder"]');

  await page.getByRole('button', { name: 'New routine' }).click();
  await page.getByPlaceholder("What's the routine?").fill('Stretch');
  await page.getByRole('button', { name: 'Daily', exact: true }).click();
  await reminder.getByRole('button', { name: '15m' }).click();
  await page.getByRole('button', { name: 'Create routine' }).click();
  await expect.poll(() => reminderOf(shell)).toBe(15);

  await page.getByText('Stretch', { exact: true }).first().click();
  await expect(
    page.getByRole('button', { name: 'Daily', exact: true }),
  ).toHaveClass(/bg-surface text-ink/);
  await expect(reminder.getByRole('button', { name: '15m' })).toHaveClass(
    /border-brand/,
  );

  await reminder.getByRole('button', { name: 'At time' }).click();
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect.poll(() => reminderOf(shell)).toBe(0);

  await page.getByText('Stretch', { exact: true }).first().click();
  await expect(reminder.getByRole('button', { name: 'At time' })).toHaveClass(
    /border-brand/,
  );
});

test('a one-off routine cannot take a reminder the server would drop', async ({
  page,
}) => {
  await openLocalRoutines(page);
  const reminder = page.locator('[data-tour="routine.reminder"]');
  await page.getByRole('button', { name: 'New routine' }).click();
  await expect(
    page.getByRole('button', { name: 'Once', exact: true }),
  ).toHaveClass(/bg-surface text-ink/);
  await expect(reminder).toContainText('Only for repeating routines');
  await expect(reminder.getByRole('button', { name: '15m' })).toBeDisabled();
  await page.getByRole('button', { name: 'Daily', exact: true }).click();
  await expect(reminder.getByRole('button', { name: '15m' })).toBeEnabled();
});
