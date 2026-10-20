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
  ['settings', '/settings', 'narrow'],
  ['settings-profile', '/settings?tab=profile', 'narrow'],
  ['settings-workspaces', '/settings/workspaces', 'narrow'],
  ['settings-features', '/settings/features', 'narrow'],
  ['settings-personas', '/settings/personas', 'narrow'],
  ['settings-plugins', '/settings/plugins', 'narrow'],
  ['plugins-help', '/settings/plugins/help', 'narrow'],
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

async function expectNarrowColumn(page) {
  const columnWidth = await page
    .locator('[data-page-header]')
    .first()
    .evaluate((h) => Math.round(h.getBoundingClientRect().width));
  expect(columnWidth).toBe(720);
}

for (const vp of VIEWPORTS) {
  for (const [name, path, width] of PAGES) {
    test(`${name} @${vp.width}: ${width} column, heading on the left edge`, async ({
      page,
    }) => {
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
      expect(Math.round(offset)).toBe(vp.edge);
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
  for (const [name, path, width] of PAGES) {
    test(`${name} @${vp.width}: ${width} page in the centered 1280 frame`, async ({
      page,
    }) => {
      await page.setViewportSize({ width: vp.width, height: vp.height });
      await mockApp(page, { taskCount: 3, multiStatus: true });
      await mockExtras(page);
      await page.goto(path);

      const heading = page.locator('[data-page-header] h1').first();
      await expect(heading).toBeVisible();

      const box = await heading.evaluate((h, sel) => {
        const main = document.querySelector('main').getBoundingClientRect();
        const frame = document.querySelector(sel).getBoundingClientRect();
        return {
          offset: h.getBoundingClientRect().left - main.left,
          expected: (main.width - 1280) / 2 + 24,
          frameWidth: frame.width,
          leftGap: frame.left - main.left,
          rightGap: main.right - frame.right,
        };
      }, `[data-page-width="${width}"]`);
      expect(Math.abs(box.offset - box.expected)).toBeLessThanOrEqual(1);
      expect(box.frameWidth).toBeLessThanOrEqual(1280);
      expect(Math.abs(box.leftGap - box.rightGap)).toBeLessThanOrEqual(1);
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
