import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const row = (id, title, slug, parent_id = null) => ({
  id,
  title,
  slug,
  type: 'plain',
  parent_id,
  position: id,
  pinned: false,
  updated_at: '2026-10-01T10:00:00Z',
});

test('All pages shows the tree: leaves have no expand button, titles align, drag re-parents', async ({
  page,
}) => {
  const moves = [];
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
  });
  await mockApp(page);
  // Leaf 3 was expanded back when leaves still had a chevron.
  await page.addInitScript(() =>
    localStorage.setItem('pages-expanded-1', JSON.stringify([3])),
  );
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
    const path = new URL(route.request().url()).pathname;
    if (route.request().method() === 'POST' && /\/move$/.test(path)) {
      moves.push(route.request().postDataJSON());
      return route.fulfill({
        json: { data: row(3, 'Leaf page', 'leaf-page') },
      });
    }
    if (/\/pages(\/tree)?$/.test(path))
      return route.fulfill({
        json: {
          data: [
            row(1, 'Parent page', 'parent-page'),
            row(2, 'Child page', 'child-page', 1),
            row(3, 'Leaf page', 'leaf-page'),
            row(4, 'Other leaf', 'other-leaf'),
          ],
        },
      });
    return route.fulfill({ json: { data: [] } });
  });

  await page.goto('/demo/pages');
  const tree = page.getByTestId('pages-index-tree');
  const parent = tree.locator('[data-page-id="1"]');
  const leaf = tree.locator('[data-page-id="3"]');
  await expect(parent).toBeVisible();
  await expect(leaf).toBeVisible();

  await expect(parent.getByRole('button', { name: 'Expand' })).toHaveCount(1);
  await expect(
    leaf.getByRole('button', { name: /Expand|Collapse/ }),
  ).toHaveCount(0);

  await expect(page.locator('[data-parent-id="3"]')).toHaveCount(0);

  await parent.getByRole('button', { name: 'Expand' }).click();
  await expect(tree.locator('[data-page-id="2"]')).toBeVisible();
  await parent.getByRole('button', { name: 'Collapse' }).click();
  await expect(tree.locator('[data-page-id="2"]')).toHaveCount(0);

  const x = async (loc) =>
    (await loc.getByText(/page$/).first().boundingBox()).x;
  expect(await x(leaf)).toBe(await x(parent));

  const from = await leaf.boundingBox();
  const target = await tree.locator('[data-page-id="4"]').boundingBox();
  await page.mouse.move(from.x + 60, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + 60, from.y + from.height / 2 + 10, {
    steps: 5,
  });
  await page.mouse.move(target.x + 60, target.y + target.height / 2, {
    steps: 10,
  });
  await page.mouse.move(target.x + 60, target.y + target.height + 4, {
    steps: 10,
  });
  await page.mouse.up();

  await expect.poll(() => moves.length).toBe(1);
  expect(moves[0].parent_id).toBe(4);
});

test('the All pages tree keeps the page menu and its rename dialog', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
  });
  await mockApp(page);
  await page.route('**/api/**/feature-toggles', (route) =>
    route.fulfill({
      json: {
        data: route.request().url().includes('/user/')
          ? {}
          : {
              board: { key: 'board', enabled: true },
              pages: { key: 'pages', enabled: true },
            },
      },
    }),
  );
  const patches = [];
  await page.route('**/api/pages**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (route.request().method() === 'PATCH') {
      patches.push(route.request().postDataJSON());
      return route.fulfill({ json: { data: row(4, 'Renamed', 'other-leaf') } });
    }
    if (/\/pages(\/tree)?$/.test(path))
      return route.fulfill({
        json: {
          data: [
            row(3, 'Leaf page', 'leaf-page'),
            row(4, 'Other leaf', 'other-leaf'),
          ],
        },
      });
    if (/\/pages\/\d+$/.test(path))
      return route.fulfill({
        json: { data: { ...row(4, 'Other leaf', 'other-leaf'), version: 3 } },
      });
    return route.fulfill({ json: { data: [] } });
  });

  await page.goto('/demo/pages');
  const node = page
    .getByTestId('pages-index-tree')
    .locator('[data-page-id="4"]');
  await node.hover();
  await node.getByRole('button', { name: 'Page actions' }).click();
  await page.getByRole('menuitem', { name: 'Rename' }).click();
  const dialog = page.getByRole('dialog', { name: 'Rename page' });
  await expect(dialog).toBeVisible();
  await dialog.getByPlaceholder('Title').fill('Renamed');
  await dialog.getByRole('button', { name: 'Save' }).click();
  await expect.poll(() => patches).toEqual([{ version: 3, title: 'Renamed' }]);
  await expect(dialog).toHaveCount(0);

  await node.hover();
  await node.getByRole('button', { name: 'Page actions' }).click();
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  await expect(
    page.getByText('"Other leaf" and all its subpages'),
  ).toBeVisible();
});
