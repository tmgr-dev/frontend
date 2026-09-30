import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const shots = process.env.BADGE_SHOTS;

const telegram = {
  folder: 'dev.telegram',
  manifest: JSON.stringify({
    id: 'dev.telegram',
    name: 'Telegram',
    version: '1.0.0',
    engines: { tmgr: '^1.3' },
    permissions: ['notifications'],
    contributes: {
      commands: [{ id: 'dev.telegram.stat', title: 'Stat clicked' }],
      views: [{ id: 'inbox', title: 'Telegram' }],
    },
  }),
  code: `
    tmgr.commands.register('dev.telegram.stat', async () => {});
    tmgr.ui.providePage('inbox', async () => ({
      type: 'stack',
      direction: 'row',
      gap: 'md',
      children: [
        { type: 'stat', label: 'Pending', value: '12', tone: 'default' },
        { type: 'stat', label: 'Pending', value: '12', tone: 'default', command: 'dev.telegram.stat' },
      ],
    }));
  `,
};

const setBadge = (page, badge) =>
  page.evaluate(async (entry) => {
    const { pluginState } = await import('/src/pluginSystem/state.ts');
    if (entry) pluginState.viewBadges['dev.telegram:inbox'] = entry;
    else delete pluginState.viewBadges['dev.telegram:inbox'];
  }, badge);

const shot = async (page, name) => {
  if (shots) await page.screenshot({ path: `${shots}/${name}.png` });
};

test('a view badge shows in the sidebar, collapses to a dot and clears', async ({
  page,
}) => {
  await page.addInitScript(() =>
    localStorage.setItem('plugins.devMode', 'true'),
  );
  await desktopPage(page, {}, { devPlugins: [telegram] });
  await page.setViewportSize({ width: 1100, height: 700 });
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);

  await page.locator('[data-sidebar="footer"] button').first().click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();
  await page
    .locator('article', { hasText: 'Telegram' })
    .getByRole('switch')
    .click();
  await page.goto('/local-personal/plugins/dev.telegram/inbox');

  const link = page.locator('[data-sidebar="menu-item"] a', {
    hasText: 'Telegram',
  });
  await expect(link).toBeVisible();
  await expect(page.getByTestId('plugin-view-badge')).toHaveCount(0);

  await setBadge(page, {
    pluginId: 'dev.telegram',
    viewId: 'inbox',
    count: 12,
    text: null,
    tone: 'danger',
  });
  const pill = page.getByTestId('plugin-view-badge');
  await expect(pill).toHaveText('12');
  await expect(link).toHaveAttribute('aria-label', 'Telegram, 12 pending');
  const red = await pill.evaluate((el) => getComputedStyle(el).color);
  expect(red).not.toBe(await link.evaluate((el) => getComputedStyle(el).color));
  await expect(page.getByTestId('plugin-view-badge-dot')).toBeHidden();
  await shot(page, 'expanded-light');

  await page.evaluate(() => document.documentElement.classList.add('dark'));
  await page.waitForTimeout(300);
  await shot(page, 'expanded-dark');

  await setBadge(page, {
    pluginId: 'dev.telegram',
    viewId: 'inbox',
    count: 150,
    text: null,
    tone: 'warning',
  });
  await expect(pill).toHaveText('99+');
  await expect(link).toHaveAttribute('aria-label', 'Telegram, 150 pending');
  await setBadge(page, {
    pluginId: 'dev.telegram',
    viewId: 'inbox',
    count: null,
    text: 'new',
    tone: 'info',
  });
  await expect(pill).toHaveText('new');
  await expect(link).toHaveAttribute('aria-label', 'Telegram, new');
  await shot(page, 'expanded-dark-text-info');
  await setBadge(page, {
    pluginId: 'dev.telegram',
    viewId: 'inbox',
    count: 12,
    text: null,
    tone: 'danger',
  });

  await page.keyboard.press('Control+b');
  await expect(page.locator('[data-collapsible="icon"]').first()).toBeVisible();
  await expect(pill).toBeHidden();
  const dot = page.getByTestId('plugin-view-badge-dot');
  await expect(dot).toBeVisible();
  await page.waitForTimeout(300);
  await shot(page, 'collapsed-dark');
  await page.evaluate(() => document.documentElement.classList.remove('dark'));
  await page.waitForTimeout(300);
  await shot(page, 'collapsed-light');

  await setBadge(page, null);
  await expect(dot).toHaveCount(0);
  await expect(link).not.toHaveAttribute('aria-label', /.+/);
  await page.keyboard.press('Control+b');
  await expect(pill).toHaveCount(0);
});

test('a clickable stat is as tall as a plain one', async ({ page }) => {
  await page.addInitScript(() =>
    localStorage.setItem('plugins.devMode', 'true'),
  );
  await desktopPage(page, {}, { devPlugins: [telegram] });
  await page.setViewportSize({ width: 1100, height: 500 });
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  await page.locator('[data-sidebar="footer"] button').first().click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();
  await page
    .locator('article', { hasText: 'Telegram' })
    .getByRole('switch')
    .click();
  await page.goto('/local-personal/plugins/dev.telegram/inbox');

  const button = page.getByRole('button').filter({ hasText: 'Pending' });
  const plain = page
    .locator('div.rounded-md.border', { hasText: 'Pending' })
    .first();
  await expect(button).toBeVisible();
  const box = async (locator) =>
    locator.evaluate((el) => ({
      height: el.getBoundingClientRect().height,
      scroll: el.scrollHeight,
      client: el.clientHeight,
    }));
  const b = await box(button);
  const p = await box(plain);
  expect(b.scroll).toBeLessThanOrEqual(b.client);
  expect(Math.abs(b.height - p.height)).toBeLessThan(1);
  await shot(page, 'stat-clickable');
});
