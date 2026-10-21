import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const PAGES = [
  ['list', '/demo/list', 'wide'],
  ['archive', '/demo/archive', 'wide'],
  ['dashboard', '/demo/dashboard', 'wide'],
  ['categories', '/demo/categories', 'wide'],
  ['notifications', '/demo/notifications', 'wide'],
  ['files', '/demo/files', 'wide'],
  ['member', '/demo/team/1', 'wide'],
  ['routines', '/routines', 'wide'],
  ['settings', '/settings', 'narrow', true],
  ['settings-profile', '/settings?tab=profile', 'narrow', true],
  ['settings-workspaces', '/settings/workspaces', 'narrow', true],
  ['settings-features', '/settings/features', 'narrow', true],
  ['settings-personas', '/settings/personas', 'narrow', true],
  ['settings-plugins', '/settings/plugins', 'narrow', true],
  ['plugins-help', '/settings/plugins/help', 'narrow', true],
  ['profile', '/profile', 'narrow'],
  ['stats', '/stats', 'wide'],
  ['pages', '/demo/pages', 'wide'],
  ['push-guide', '/push-notifications-enable-guide', 'narrow'],
];

const VIEWPORTS = [
  { width: 1440, height: 900, edge: 24 },
  { width: 390, height: 844, edge: 16 },
];

async function mockExtras(page) {
  await page.route(
    (u) =>
      u.pathname.startsWith('/api/') &&
      u.pathname.endsWith('feature-toggles') &&
      !u.pathname.includes('/user'),
    (r) =>
      r.fulfill({
        json: {
          data: {
            board: { enabled: true },
            dashboard: { enabled: true },
            categories: { enabled: true },
            daily_routines: { enabled: true },
            pages: { enabled: true },
          },
        },
      }),
  );
  await page.route(
    (u) => u.pathname.endsWith('/api/project_categories'),
    (r) =>
      r.fulfill({
        json: {
          data: [
            {
              id: 1,
              title: 'Work',
              code: 'WORK',
              project_category_id: null,
              deleted_at: null,
              settings: [],
            },
          ],
        },
      }),
  );
  await page.route(
    (u) => u.pathname.startsWith('/api/') && u.pathname.includes('/dashboard/'),
    (r) => {
      const p = new URL(r.request().url()).pathname;
      if (p.endsWith('activities') || p.endsWith('/tasks')) {
        return r.fulfill({
          json: {
            data: [],
            meta: { current_page: 1, per_page: 20, total: 0, last_page: 1 },
            links: {},
          },
        });
      }
      if (/members\/\d+$/.test(p)) {
        return r.fulfill({
          json: {
            data: { id: 1, name: 'Test User', email: 'test@example.test' },
          },
        });
      }
      return r.fulfill({ json: { data: {} } });
    },
  );
}

// Settings pages (D-011): a 208px settings nav column and the page sit side by
// side as one centred max-w-7xl unit, so on desktop the heading starts 208px
// further right than on a plain page.
const SETTINGS_NAV = 208;
const SETTINGS_UNIT = '[data-testid="settings-layout"]';

async function expectNarrowColumn(page) {
  const columnWidth = await page
    .locator('[data-page-header]')
    .first()
    .evaluate((h) => Math.round(h.getBoundingClientRect().width));
  expect(columnWidth).toBe(720);
}

async function expectSettingsNav(page) {
  const navWidth = await page
    .locator(`${SETTINGS_UNIT} > aside`)
    .evaluate((a) => Math.round(a.getBoundingClientRect().width));
  expect(navWidth).toBe(SETTINGS_NAV);
  await expect(
    page.getByRole('navigation', { name: 'Settings' }),
  ).toBeVisible();
}

for (const vp of VIEWPORTS) {
  for (const [name, path, width, settings] of PAGES) {
    const nav = settings && vp.width >= 1280 ? SETTINGS_NAV : 0;
    test(`${name} @${vp.width}: ${width} column, heading ${
      nav ? 'right of the settings nav' : 'on the left edge'
    }`, async ({ page }) => {
      await page.setViewportSize({ width: vp.width, height: vp.height });
      await mockApp(page, { taskCount: 3, multiStatus: true });
      await mockExtras(page);
      await page.goto(path);

      const container = page.locator(`[data-page-width="${width}"]`).first();
      const heading = page.locator('[data-page-header] h1').first();
      await expect(heading).toBeVisible();

      const offset = await heading.evaluate((h) => {
        const main = document.querySelector('main');
        return (
          h.getBoundingClientRect().left - main.getBoundingClientRect().left
        );
      });
      expect(Math.round(offset)).toBe(vp.edge + nav);
      if (nav) await expectSettingsNav(page);
      if (settings && !nav)
        await expect(page.locator(SETTINGS_UNIT)).toHaveCount(0);
      await expect(container).toBeVisible();
      if (width === 'narrow' && vp.width >= 768) await expectNarrowColumn(page);

      const overflow = await page.evaluate(
        () =>
          document.documentElement.scrollWidth >
          document.documentElement.clientWidth,
      );
      expect(overflow).toBe(false);
    });
  }
}

const WIDE_VIEWPORTS = [
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
];

for (const vp of WIDE_VIEWPORTS) {
  for (const [name, path, width, settings] of PAGES) {
    const nav = settings ? SETTINGS_NAV : 0;
    test(`${name} @${vp.width}: ${width} page in the centered 1280 ${
      settings ? 'settings unit' : 'frame'
    }`, async ({ page }) => {
      await page.setViewportSize({ width: vp.width, height: vp.height });
      await mockApp(page, { taskCount: 3, multiStatus: true });
      await mockExtras(page);
      await page.goto(path);

      const heading = page.locator('[data-page-header] h1').first();
      await expect(heading).toBeVisible();

      const box = await heading.evaluate(
        (h, { sel, nav }) => {
          const main = document.querySelector('main').getBoundingClientRect();
          const frame = document.querySelector(sel).getBoundingClientRect();
          return {
            offset: h.getBoundingClientRect().left - main.left,
            expected: (main.width - 1280) / 2 + 24 + nav,
            frameWidth: frame.width,
            leftGap: frame.left - main.left,
            rightGap: main.right - frame.right,
          };
        },
        {
          sel: settings ? SETTINGS_UNIT : `[data-page-width="${width}"]`,
          nav,
        },
      );
      expect(Math.abs(box.offset - box.expected)).toBeLessThanOrEqual(1);
      expect(box.frameWidth).toBeLessThanOrEqual(1280);
      expect(Math.abs(box.leftGap - box.rightGap)).toBeLessThanOrEqual(1);
      if (settings) await expectSettingsNav(page);
      if (width === 'narrow') await expectNarrowColumn(page);

      const overflow = await page.evaluate(
        () =>
          document.documentElement.scrollWidth >
          document.documentElement.clientWidth,
      );
      expect(overflow).toBe(false);
    });
  }
}

test('/settings without a tab opens the workspace settings page', async ({
  page,
}) => {
  await mockApp(page);
  await mockExtras(page);
  await page.goto('/settings');
  await expect(page).toHaveURL(/\/settings\/workspaces$/);
  await expect(page.locator('[data-page-header] h1').first()).toHaveText(
    'Workspace settings',
  );
});
