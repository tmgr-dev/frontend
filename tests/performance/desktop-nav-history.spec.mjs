import { expect, test } from '@playwright/test';

import { desktopPage } from './desktopShell.mjs';

test('desktop back/forward walks routes via header buttons and Cmd+[ / Cmd+]', async ({
  page,
}) => {
  await page.addInitScript(() =>
    Object.defineProperty(navigator, 'platform', { get: () => 'MacIntel' }),
  );
  await page.setViewportSize({ width: 1440, height: 800 });
  await desktopPage(page, { taskCount: 3 });
  await page.goto('/demo/dashboard');
  const back = page.getByRole('button', { name: 'Back', exact: true });
  const forward = page.getByRole('button', { name: 'Forward', exact: true });
  await back.waitFor();
  await expect(page).toHaveURL(/\/demo\/dashboard/);
  await page.waitForLoadState('networkidle');
  await expect(back).toBeDisabled();
  await expect(forward).toBeDisabled();

  await page.evaluate(() => {
    const vue = document.querySelector('#app').__vue_app__;
    vue.config.globalProperties.$router.push('/settings/workspaces');
  });
  await expect(page).toHaveURL(/\/settings\/workspaces/);
  await expect(back).toBeEnabled();

  await back.click();
  await expect(page).toHaveURL(/\/demo\/dashboard/);
  await expect(forward).toBeEnabled();
  await expect(back).toBeDisabled();

  await page.keyboard.press('Meta+]');
  await expect(page).toHaveURL(/\/settings\/workspaces/);
  await page.keyboard.press('Meta+[');
  await expect(page).toHaveURL(/\/demo\/dashboard/);

  await page.evaluate(() => window.__emit('menu://nav-forward'));
  await expect(page).toHaveURL(/\/settings\/workspaces/);
  await page.evaluate(() => window.__emit('menu://nav-back'));
  await expect(page).toHaveURL(/\/demo\/dashboard/);
});
