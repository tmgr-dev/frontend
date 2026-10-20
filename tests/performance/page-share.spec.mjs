import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';

import { mockApp } from './mockApp.mjs';

test.setTimeout(90_000);

const author = { kind: 'user', id: 1, name: 'Test User' };

const seed = [
  {
    id: 1,
    title: 'Handbook',
    slug: 'handbook',
    body: 'Plain intro paragraph.\n',
  },
  {
    id: 2,
    title: 'Onboarding',
    slug: 'onboarding',
    body: 'Back to the handbook.\n',
  },
  { id: 3, title: 'Plan Q4', slug: 'план q4', body: 'Quarter.\n' },
];

const summary = (p) => ({
  id: p.id,
  title: p.title,
  slug: p.slug,
  type: 'plain',
  parent_id: null,
  position: p.id,
  pinned: false,
  updated_at: '2026-10-01T10:00:00Z',
});

const full = (p) => ({
  ...summary(p),
  workspace_id: 1,
  body: p.body,
  properties: {},
  version: 1,
  author,
  updated_by: author,
  created_at: '2026-10-01T10:00:00Z',
  backlinks: [],
  sections: [],
  files: [],
  following: false,
});

const mockPages = async (page) => {
  await page.route('**/api/**/feature-toggles', (route) => {
    const toggle = (key) => ({
      key,
      name: key,
      group: 'pages',
      type: 'boolean',
      enabled: true,
    });
    return route.fulfill({
      json: {
        data: route.request().url().includes('/user/')
          ? {}
          : { board: toggle('board'), pages: toggle('pages') },
      },
    });
  });
  await page.route('**/api/pages**', (route) => {
    const path = decodeURIComponent(
      new URL(route.request().url()).pathname
        .replace('/api/pages', '')
        .replace(/^\//, ''),
    );
    const json = (data) => route.fulfill({ json: { data } });
    if (path === 'tree' || path === '') return json(seed.map(summary));
    const target = seed.find((p) => String(p.id) === path || p.slug === path);
    return target ? json(full(target)) : json([]);
  });
};

const pageErrors = (page) => {
  const errors = [];
  page.on('pageerror', (error) => {
    if (/graph/i.test(`${error.message}\n${error.stack}`)) return;
    errors.push(error.message);
  });
  return errors;
};

const prepare = async (page, context) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    window.__clipboardCalls = [];
    for (const name of ['write', 'writeText']) {
      const original = Clipboard.prototype[name];
      Clipboard.prototype[name] = function (...args) {
        window.__clipboardCalls.push(name);
        return original.apply(this, args);
      };
    }
  });
  await mockApp(page);
  await mockPages(page);
  return pageErrors(page);
};

const clipboard = (page) => page.evaluate(() => navigator.clipboard.readText());
const clipboardCalls = (page) => page.evaluate(() => window.__clipboardCalls);

const openPageMenu = async (page) => {
  await page.getByTestId('page-more-menu').click();
  await page.getByTestId('page-share-submenu').click();
};

const openTreeMenu = async (page, id) => {
  const row = page
    .getByTestId('pages-sidebar-section')
    .locator(`[data-page-id="${id}"]`);
  await row.hover();
  await row.getByRole('button', { name: 'Page actions' }).click();
  return page.getByRole('menu');
};

test('page menu Share copies web link, app link and the Markdown export', async ({
  page,
  context,
}) => {
  const errors = await prepare(page, context);
  await page.goto('/demo/pages/onboarding');
  await expect(page.getByTestId('page-more-menu')).toBeVisible();

  await openPageMenu(page);
  await page.getByTestId('page-share-copy-link').click();
  await expect
    .poll(() => clipboard(page))
    .toBe('https://tmgr.dev/demo/pages/onboarding');
  await expect(page.getByText('Link copied').first()).toBeVisible();

  await openPageMenu(page);
  await page.getByTestId('page-share-copy-app-link').click();
  await expect.poll(() => clipboard(page)).toBe('tmgr://page/demo/onboarding');

  await page.getByTestId('page-more-menu').click();
  const download = page.waitForEvent('download');
  await page.getByTestId('page-export-md').click();
  const exported = readFileSync(await (await download).path(), 'utf8');
  expect(exported).toMatch(/^---\n/);
  expect(exported).toContain('title: Onboarding');
  expect(exported).toContain('Back to the handbook.');

  await openPageMenu(page);
  await page.getByTestId('page-share-copy-markdown').click();
  await expect(page.getByText('Markdown copied').first()).toBeVisible();
  expect(await clipboard(page)).toBe(exported);

  await openPageMenu(page);
  await expect(page.getByTestId('page-share-open-app')).toBeVisible();
  expect(errors).toEqual([]);
});

test('desktop page menu does not offer Open in TMGR app', async ({
  page,
  context,
}) => {
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      invoke: () => Promise.resolve(null),
      transformCallback: () => 0,
    };
  });
  await prepare(page, context);
  await page.goto('/demo/pages/onboarding');
  await openPageMenu(page);
  await expect(page.getByTestId('page-share-copy-app-link')).toBeVisible();
  await expect(page.getByTestId('page-share-open-app')).toHaveCount(0);
});

test('tree item Share copies the link and the fetched page as Markdown via ClipboardItem', async ({
  page,
  context,
}) => {
  const errors = await prepare(page, context);
  await page.goto('/demo/pages/handbook');
  await expect(page.getByTestId('page-more-menu')).toBeVisible();

  await openTreeMenu(page, 2);
  await page.getByTestId('page-share-submenu').click();
  await page.getByTestId('page-share-copy-link').click();
  await expect
    .poll(() => clipboard(page))
    .toBe('https://tmgr.dev/demo/pages/onboarding');

  await page.evaluate(() => (window.__clipboardCalls = []));
  await openTreeMenu(page, 2);
  await page.getByTestId('page-share-submenu').click();
  await page.getByTestId('page-share-copy-markdown').click();
  await expect(page.getByText('Markdown copied').first()).toBeVisible();
  const text = await clipboard(page);
  expect(text).toMatch(/^---\n/);
  expect(text).toContain('title: Onboarding');
  expect(text).toContain('Back to the handbook.');
  expect(await clipboardCalls(page)).toEqual(['write']);
  expect(errors).toEqual([]);
});

test('tree item with a slug the deep link rejects offers only the web link', async ({
  page,
  context,
}) => {
  await prepare(page, context);
  await page.goto('/demo/pages/handbook');
  await openTreeMenu(page, 3);
  await page.getByTestId('page-share-submenu').click();
  await expect(page.getByTestId('page-share-copy-link')).toBeVisible();
  await expect(page.getByTestId('page-share-copy-markdown')).toBeVisible();
  await expect(page.getByTestId('page-share-copy-app-link')).toHaveCount(0);
  await expect(page.getByTestId('page-share-open-app')).toHaveCount(0);
  await page.getByTestId('page-share-copy-link').click();
  await expect
    .poll(() => clipboard(page))
    .toBe('https://tmgr.dev/demo/pages/%D0%BF%D0%BB%D0%B0%D0%BD%20q4');
});

test('tree Copy as Markdown shows an error toast when the page fetch fails', async ({
  page,
  context,
}) => {
  const errors = await prepare(page, context);
  await page.goto('/demo/pages/handbook');
  await expect(page.getByTestId('page-more-menu')).toBeVisible();
  await page.evaluate(() => navigator.clipboard.writeText('before'));
  await page.route('**/api/pages/2', (route) =>
    route.fulfill({ status: 404, json: { message: 'Page not found' } }),
  );

  await openTreeMenu(page, 2);
  await page.getByTestId('page-share-submenu').click();
  await page.getByTestId('page-share-copy-markdown').click();
  await expect(
    page.getByText("Couldn't copy the Markdown").first(),
  ).toBeVisible();
  await expect(page.getByText('Markdown copied')).toHaveCount(0);
  expect(await clipboard(page)).toBe('before');
  await page.waitForTimeout(300);
  expect(errors).toEqual([]);
});

const rowMetrics = (row) =>
  row.evaluate((el) => {
    const icon = el.querySelector('svg').getBoundingClientRect();
    const label = el.querySelector('span').getBoundingClientRect();
    return {
      iconX: Math.round(icon.left),
      iconW: Math.round(icon.width),
      gap: Math.round(label.left - icon.right),
    };
  });

const expectAligned = async (subTrigger, plainItem) => {
  await subTrigger
    .page()
    .waitForFunction(() =>
      document.getAnimations().every((a) => a.playState !== 'running'),
    );
  const sub = await rowMetrics(subTrigger);
  const plain = await rowMetrics(plainItem);
  expect(plain.iconW).toBe(16);
  expect(plain.gap).toBeGreaterThanOrEqual(12);
  expect(sub.iconW).toBe(plain.iconW);
  expect(Math.abs(sub.iconX - plain.iconX)).toBeLessThanOrEqual(1);
  expect(Math.abs(sub.gap - plain.gap)).toBeLessThanOrEqual(1);
};

test('submenu rows line up with plain items in page, tree, board and list menus', async ({
  page,
  context,
}) => {
  await prepare(page, context);
  await page.goto('/demo/pages/handbook');

  await page.getByTestId('page-more-menu').click();
  await expectAligned(
    page.getByTestId('page-share-submenu'),
    page.getByTestId('page-export-md'),
  );
  await page.keyboard.press('Escape');

  const tree = await openTreeMenu(page, 2);
  const rename = tree.getByRole('menuitem', { name: 'Rename', exact: true });
  await expectAligned(page.getByTestId('page-share-submenu'), rename);
  await expectAligned(
    tree.getByRole('menuitem', { name: 'Add subpage' }),
    rename,
  );
  await page.keyboard.press('Escape');

  await page.route('**/api/tasks/status/**', (route) =>
    route.fulfill({
      json: {
        data: [1, 2].map((id) => ({
          id,
          title: `Task ${id}`,
          status_id: 1,
          workspace_id: 1,
          user_id: 1,
          assignees: [],
          settings: [],
          checkpoints: [],
          common_time: 0,
          start_time: 0,
          approximately_time: 0,
          project_category_id: null,
          created_at: '2026-09-13T10:00:00Z',
          updated_at: '2026-09-13T10:00:00Z',
        })),
      },
    }),
  );
  await page.goto('/demo/board');
  const card = page.locator('[data-task-id="2"]');
  await card.hover();
  await card.getByRole('button', { name: 'Task actions' }).click();
  await expectAligned(
    page.getByTestId('task-share-submenu'),
    page.getByRole('menuitem', { name: 'Open details' }),
  );
  await page.keyboard.press('Escape');

  await page.goto('/demo/list');
  const row = page.locator('[data-task-id="1"]').first();
  await row.hover();
  await row.getByRole('button', { name: 'Task actions', exact: true }).click();
  await expectAligned(
    page.getByTestId('task-share-submenu'),
    page.getByRole('menuitem', { name: 'Open details' }),
  );
});
