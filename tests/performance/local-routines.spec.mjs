import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const shotsDir = process.env.SHOTS_DIR;

const screenshot = async (page, name) => {
  if (!shotsDir) return;
  await page.screenshot({ path: `${shotsDir}/${name}.png` });
};

test('local routines are created and completed entirely offline, clearly labelled as local', async ({
  page,
}) => {
  const shell = await desktopPage(page);
  const routineRequests = [];
  page.on('request', (request) => {
    const url = new URL(request.url());
    if (!url.pathname.startsWith('/api/')) return;
    const path = url.pathname.replace('/api/', '');
    if (path.startsWith('daily-routines')) routineRequests.push(path);
  });

  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  routineRequests.length = 0;

  await expect(page.getByTestId('local-routines-menu-badge')).toBeVisible();
  await screenshot(page, 'menu-light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await screenshot(page, 'menu-dark');
  await page.emulateMedia({ colorScheme: 'light' });

  await page.goto('/routines');
  await expect(page.getByTestId('local-routines-label')).toBeVisible();
  await expect(page.getByText('Local routines').first()).toBeVisible();
  await expect(page).toHaveTitle('Local routines | TMGR');
  await expect(page.getByTitle('Import .ics calendar')).toHaveCount(0);
  await expect(page.getByTitle('Export .ics calendar')).toHaveCount(0);
  await screenshot(page, 'page-light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await screenshot(page, 'page-dark');
  await page.emulateMedia({ colorScheme: 'light' });

  await page.setViewportSize({ width: 375, height: 812 });
  await expect(page.getByTestId('local-routines-label')).toBeVisible();
  await screenshot(page, 'page-phone');
  await page.setViewportSize({ width: 1280, height: 800 });

  await page.getByPlaceholder(/Quick add/).fill('Water plants');
  await page.keyboard.press('Enter');
  await expect(page.getByText('Water plants').first()).toBeVisible();

  await page.getByRole('button', { name: 'New routine' }).click();
  await page.getByPlaceholder("What's the routine?").fill('Morning stretch');
  await page.getByRole('button', { name: 'Daily', exact: true }).click();
  await page.getByRole('button', { name: 'Create routine' }).click();
  await expect(page.getByText('Morning stretch').first()).toBeVisible();

  await expect
    .poll(async () =>
      (await shell('local_db_select', {
        code: 'personal',
        sql: 'SELECT title FROM routines ORDER BY id',
        params: [],
      })).map((row) => row.title),
    )
    .toEqual(expect.arrayContaining(['Water plants', 'Morning stretch']));

  const waterTitle = page.getByText('Water plants', { exact: true });
  await waterTitle.locator('xpath=../..').locator('button').first().click();

  await expect
    .poll(async () =>
      (await shell('local_db_select', {
        code: 'personal',
        sql: "SELECT COUNT(*) as n FROM routine_instances WHERE status = 'COMPLETED'",
        params: [],
      }))[0].n,
    )
    .toBeGreaterThan(0);

  expect(routineRequests).toEqual([]);
});
