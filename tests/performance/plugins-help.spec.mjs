import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
  });
});

const openPlugins = async (page) => {
  await page.goto('/demo/board');
  await page.getByTestId('workspace-rail-user').click();
  await page.getByRole('menuitem', { name: 'Settings' }).click();
  await page.getByTestId('settings-nav-plugins').click();
};

test('the plugins screen links to a page that explains how plugins work', async ({
  page,
}) => {
  await desktopPage(page);
  await openPlugins(page);
  await expect(
    page.getByText('Can: read tasks, read statuses, read tracked time'),
  ).toBeVisible();
  await page.getByRole('link', { name: 'How plugins work' }).click();
  await expect(page).toHaveURL(/\/settings\/plugins\/help$/);
  for (const heading of [
    'What a plugin can do',
    'Installing from GitHub',
    'Plugins without a signature',
    'Shared workspaces',
    'When a plugin gets in the way',
    'For plugin authors',
  ]) {
    await expect(page.getByRole('heading', { name: heading })).toBeVisible();
  }
  await expect(
    page.getByRole('link', { name: /tmgr-plugin-template/ }),
  ).toHaveAttribute('href', 'https://github.com/tmgr-dev/tmgr-plugin-template');
  await page.getByRole('link', { name: /Back to plugins/ }).click();
  await expect(page).toHaveURL(/\/settings\/plugins$/);
});

test('the help page fits a phone screen', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await desktopPage(page);
  await page.goto('/demo/board');
  await page.goto('/settings/plugins/help');
  await expect(
    page.getByRole('heading', { name: 'How plugins work' }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth > window.innerWidth,
    ),
  ).toBe(false);
});
