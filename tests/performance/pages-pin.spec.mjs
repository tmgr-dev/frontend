import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const row = (id, title, parent_id = null, extra = {}) => ({
  id,
  title,
  slug: `page-${id}`,
  type: 'plain',
  parent_id,
  position: id,
  pinned: false,
  updated_at: `2026-10-01T10:${String(id).padStart(2, '0')}:00Z`,
  ...extra,
});

// Stateful: pin/unpin flip `pinned` and bump `updated_at`, the next tree GET reflects it.
async function mockPages(page, initial, { treeStatus = 200 } = {}) {
  const pages = initial.map((p) => ({ ...p }));
  const calls = [];
  let clock = Date.parse('2026-10-20T10:00:00Z');
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
  });
  await mockApp(page);
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
    const method = route.request().method();
    const path = new URL(route.request().url()).pathname.replace('/api/', '');
    calls.push(`${method} ${path}`);
    const pin = path.match(/^pages\/(\d+)\/(pin|unpin)$/);
    if (method === 'POST' && pin) {
      const target = pages.find((p) => p.id === Number(pin[1]));
      target.pinned = pin[2] === 'pin';
      clock += 60000;
      target.updated_at = new Date(clock).toISOString();
      return route.fulfill({ json: { data: { ...target } } });
    }
    if (method === 'GET' && /^pages(\/tree)?$/.test(path))
      return treeStatus === 200
        ? route.fulfill({ json: { data: pages.map((p) => ({ ...p })) } })
        : route.fulfill({ status: treeStatus, json: { message: 'boom' } });
    return route.fulfill({ json: { data: [] } });
  });
  return { pages, calls };
}

const section = (page) => page.getByTestId('pages-sidebar-section');
const rowIds = (page) =>
  section(page)
    .locator('[data-page-id]')
    .evaluateAll((els) => els.map((el) => Number(el.dataset.pageId)));

const openMenu = async (page, id) => {
  const rowEl = section(page).locator(`[data-page-id="${id}"]`);
  await expect(rowEl).toBeVisible({ timeout: 5000 });
  await rowEl.hover();
  await rowEl.getByRole('button', { name: 'Page actions' }).click();
};

const pinIcon = (page, id) =>
  section(page).locator(`[data-page-id="${id}"] .lucide-pin`);

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
});

test('the sidebar lists pinned pages, then the 10 latest edits flat', async ({
  page,
}) => {
  const tree = [
    row(1, 'Handbook'),
    row(2, 'Pinned child', 1, { pinned: true }),
    row(3, 'Roadmap', null, { pinned: true }),
    ...Array.from({ length: 12 }, (_, i) =>
      row(i + 4, `Note ${i + 4}`, i % 3 === 0 ? 1 : null),
    ),
  ];
  await mockPages(page, tree);
  await page.goto('/demo/board');
  await expect(section(page).locator('[data-page-id="3"]')).toBeVisible();

  expect(await rowIds(page)).toEqual([
    3, 2, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6,
  ]);
  await expect(pinIcon(page, 3)).toHaveCount(1);
  await expect(pinIcon(page, 2)).toHaveCount(1);
  await expect(pinIcon(page, 15)).toHaveCount(0);
  await expect(
    section(page).getByRole('button', { name: /Expand|Collapse/ }),
  ).toHaveCount(0);
  await expect(section(page).locator('[data-parent-id]')).toHaveCount(0);
  const x = async (id) =>
    (await section(page).locator(`[data-page-id="${id}"] a`).boundingBox()).x;
  expect(await x(2)).toBe(await x(15));

  await expect(
    section(page).getByRole('link', { name: 'All pages' }),
  ).toBeVisible();
  await expect(
    section(page).getByRole('link', { name: 'Archive' }),
  ).toBeVisible();
  await expect(
    section(page).getByRole('button', { name: 'New page' }),
  ).toBeVisible();
  await expect(page.locator('[data-tour="pages.type-picker"]')).toHaveCount(1);
  await expect(page.getByTestId('sidebar-pinned')).toHaveCount(0);

  // Every row's "…" stays hidden until hover (radix as-child used to strip the class from all rows but the first).
  for (const id of [3, 2, 15, 6])
    await expect(
      section(page)
        .locator(`[data-page-id="${id}"]`)
        .getByRole('button', { name: 'Page actions' }),
      `row ${id}`,
    ).toHaveCSS('opacity', '0');

  await openMenu(page, 15);
  for (const name of [
    'Add subpage',
    'Rename',
    'Pin',
    'Export with subpages',
    'Import Markdown here…',
    'Archive',
  ])
    await expect(
      page.getByRole('menuitem', { name, exact: true }),
    ).toBeVisible();
  await expect(page.getByTestId('page-export-subtree')).toBeVisible();
  await expect(page.getByTestId('page-import-here')).toBeVisible();
  await page.keyboard.press('Escape');

  await section(page).locator('[data-page-id="15"] a').click();
  await expect(page).toHaveURL(/\/demo\/pages\/page-15$/);
  await expect(section(page).locator('[data-page-id="15"]')).toHaveClass(
    /bg-sidebar-accent/,
  );
});

test('pin moves a recent page to the top, unpin sends it back to the recents', async ({
  page,
}) => {
  const api = await mockPages(page, [
    row(1, 'Alpha', null, { pinned: true }),
    row(2, 'Beta'),
    row(3, 'Gamma'),
  ]);
  await page.goto('/demo/board');
  await expect(section(page).locator('[data-page-id="1"]')).toBeVisible();
  expect(await rowIds(page)).toEqual([1, 3, 2]);

  await openMenu(page, 2);
  await page.getByRole('menuitem', { name: 'Pin', exact: true }).click();
  await expect.poll(() => api.calls).toContain('POST pages/2/pin');
  await expect.poll(() => rowIds(page)).toEqual([1, 2, 3]);
  await expect(pinIcon(page, 2)).toHaveCount(1);

  await openMenu(page, 1);
  await page.getByRole('menuitem', { name: 'Unpin' }).click();
  await expect.poll(() => api.calls).toContain('POST pages/1/unpin');
  await expect.poll(() => rowIds(page)).toEqual([2, 1, 3]);
  await expect(
    section(page)
      .locator('[data-page-id="1"]')
      .getByRole('button', { name: 'Page actions' }),
    'focus follows the moved row',
  ).toBeFocused();
  await expect(pinIcon(page, 1)).toHaveCount(0);
});

// Regression: on develop a pinned subpage under a collapsed parent showed only in the
// top "Pinned" group, which had no menu, so it could not be unpinned from the sidebar.
test('a pinned subpage under a collapsed parent can be unpinned from the sidebar', async ({
  page,
}) => {
  const api = await mockPages(page, [
    row(1, 'Parent'),
    row(2, 'Pinned child', 1, { pinned: true }),
    row(3, 'Other'),
  ]);
  await page.goto('/demo/board');

  await openMenu(page, 2);
  await page.getByRole('menuitem', { name: 'Unpin' }).click();
  await expect.poll(() => api.calls).toContain('POST pages/2/unpin');
  await expect(pinIcon(page, 2)).toHaveCount(0);
});

test('pin and unpin on the All pages tree update the sidebar without a reload', async ({
  page,
}) => {
  const api = await mockPages(page, [
    row(1, 'Alpha'),
    row(2, 'Beta', 1),
    row(3, 'Gamma'),
  ]);
  await page.goto('/demo/pages');
  const tree = page.getByTestId('pages-index-tree');
  const node = tree.locator('[data-page-id="1"]');
  await expect(node).toBeVisible();
  await expect.poll(() => rowIds(page)).toEqual([3, 2, 1]);

  const pick = async (name) => {
    await node.hover();
    await node.getByRole('button', { name: 'Page actions' }).click();
    await page.getByRole('menuitem', { name, exact: true }).click();
  };
  await pick('Pin');
  await expect.poll(() => api.calls).toContain('POST pages/1/pin');
  await expect(pinIcon(page, 1)).toHaveCount(1);
  await expect.poll(() => rowIds(page)).toEqual([1, 3, 2]);

  await pick('Unpin');
  await expect.poll(() => api.calls).toContain('POST pages/1/unpin');
  await expect(pinIcon(page, 1)).toHaveCount(0);
});

test('an empty workspace shows only the section header and links', async ({
  page,
}) => {
  await mockPages(page, []);
  await page.goto('/demo/board');
  await expect(
    section(page).getByRole('link', { name: 'All pages' }),
  ).toBeVisible();
  await expect(section(page).locator('[data-page-id]')).toHaveCount(0);
  await expect(section(page).getByText('Loading…')).toHaveCount(0);
  await expect(section(page).getByText('Failed to load pages')).toHaveCount(0);
});

test('a failing tree shows the error in the sidebar and on All pages', async ({
  page,
}) => {
  await mockPages(page, [row(1, 'Alpha')], { treeStatus: 500 });
  await page.goto('/demo/pages');
  await expect(section(page).getByText('Failed to load pages')).toBeVisible({
    timeout: 15000,
  });
  await expect(
    page.getByTestId('pages-index').getByText('Failed to load pages'),
  ).toBeVisible();
});

test.describe('touch phone', () => {
  test.use({ hasTouch: true });

  // Touch has no hover: the row menu (the only Pin/Unpin control) must not stay at opacity 0.
  test('the row menu button is visible in the sheet without hover', async ({
    page,
  }) => {
    await mockPages(page, [
      row(1, 'Alpha', null, { pinned: true }),
      row(2, 'Beta'),
    ]);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/demo/board');
    await page.waitForLoadState('networkidle');
    await page.locator('[data-tour="nav.menu-button"]').first().tap();
    await expect(section(page).locator('[data-page-id="1"]')).toBeVisible();
    for (const id of [1, 2]) {
      const button = section(page)
        .locator(`[data-page-id="${id}"]`)
        .getByRole('button', { name: 'Page actions' });
      await expect(button, `row ${id}`).toBeVisible();
      await expect(button, `row ${id}`).not.toHaveCSS('opacity', '0');
    }
  });
});
