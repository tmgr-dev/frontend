import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const PAGES = [
  ['list', '/demo/list', 'wide'],
  ['archive', '/demo/archive', 'wide'],
  ['dashboard', '/demo/dashboard', 'wide'],
  ['categories', '/demo/categories', 'wide'],
  ['category-create', '/demo/categories/create', 'narrow'],
  ['notifications', '/demo/notifications', 'wide'],
  ['files', '/demo/files', 'wide'],
  ['member', '/demo/team/1', 'wide'],
  ['routines', '/routines', 'wide'],
  ['settings', '/settings', 'narrow'],
  ['settings-workspaces', '/settings/workspaces', 'narrow'],
  ['settings-features', '/settings/features', 'narrow'],
  ['settings-personas', '/settings/personas', 'narrow'],
  ['settings-plugins', '/settings/plugins', 'narrow'],
  ['plugins-help', '/settings/plugins/help', 'narrow'],
  ['profile', '/profile', 'narrow'],
  ['stats', '/stats', 'wide'],
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
          },
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
          json: { data: { id: 1, name: 'Test User', email: 'test@example.test' } },
        });
      }
      return r.fulfill({ json: { data: {} } });
    },
  );
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
        return h.getBoundingClientRect().left - main.getBoundingClientRect().left;
      });
      expect(Math.round(offset)).toBe(vp.edge);
      await expect(container).toBeVisible();

      const overflow = await page.evaluate(
        () =>
          document.documentElement.scrollWidth >
          document.documentElement.clientWidth,
      );
      expect(overflow).toBe(false);
    });
  }
}
