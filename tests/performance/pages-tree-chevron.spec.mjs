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

test('leaf pages have no expand button and titles stay aligned', async ({
  page,
}) => {
  const moves = [];
  await page.setViewportSize({ width: 1400, height: 900 });
  await mockApp(page);
  // Leaf 3 was expanded back when leaves still had a chevron.
  await page.addInitScript(() =>
    localStorage.setItem('pages-expanded-1', JSON.stringify([3])),
  );
  await page.route('**/api/**/feature-toggles', (route) => {
    const toggle = (key) => ({ key, name: key, group: 'pages', type: 'boolean', enabled: true });
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
      return route.fulfill({ json: { data: row(3, 'Leaf page', 'leaf-page') } });
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
  const parent = page.locator('[data-page-id="1"]');
  const leaf = page.locator('[data-page-id="3"]');
  await expect(parent).toBeVisible();
  await expect(leaf).toBeVisible();

  await expect(parent.getByRole('button', { name: 'Expand' })).toHaveCount(1);
  await expect(
    leaf.getByRole('button', { name: /Expand|Collapse/ }),
  ).toHaveCount(0);

  await expect(page.locator('[data-parent-id="3"]')).toHaveCount(0);

  const x = async (loc) => (await loc.getByText(/page$/).first().boundingBox()).x;
  expect(await x(leaf)).toBe(await x(parent));

  const from = await leaf.boundingBox();
  const target = await page.locator('[data-page-id="4"]').boundingBox();
  await page.mouse.move(from.x + 60, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + 60, from.y + from.height / 2 + 10, { steps: 5 });
  await page.mouse.move(target.x + 60, target.y + target.height / 2, { steps: 10 });
  await page.mouse.move(target.x + 60, target.y + target.height + 4, { steps: 10 });
  await page.mouse.up();

  await expect.poll(() => moves.length).toBe(1);
  expect(moves[0].parent_id).toBe(4);
});
