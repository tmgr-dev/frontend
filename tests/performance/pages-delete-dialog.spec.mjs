import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const shotsDir = process.env.DELETE_DIALOG_SHOTS_DIR;

const row = (id, title, parent_id = null) => ({
  id,
  title,
  slug: `page-${id}`,
  type: 'plain',
  parent_id,
  position: id,
  pinned: false,
  updated_at: `2026-10-01T10:${String(id).padStart(2, '0')}:00Z`,
});

const LONG_TITLE = 'Quarterly'.repeat(18);

const initialPages = () => [
  row(1, 'Handbook'),
  row(2, 'Onboarding', 1),
  row(3, 'Roadmap'),
  row(4, LONG_TITLE),
];

// Stateful: DELETE drops the page and its subtree, the next tree GET reflects it.
async function mockPages(page, { deleteStatus = 200, scheme = 'light' } = {}) {
  const pages = initialPages();
  const trash = [];
  const calls = [];
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.addInitScript((value) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
    localStorage.setItem('colorScheme', value);
  }, scheme);
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
    const del = path.match(/^pages\/(\d+)$/);
    if (method === 'DELETE' && del) {
      if (deleteStatus !== 200)
        return route.fulfill({
          status: deleteStatus,
          json: { message: 'Cannot delete' },
        });
      const gone = new Set([Number(del[1])]);
      for (let grew = true; grew;) {
        grew = false;
        for (const p of pages)
          if (gone.has(p.parent_id) && !gone.has(p.id)) {
            gone.add(p.id);
            grew = true;
          }
      }
      for (let i = pages.length - 1; i >= 0; i--)
        if (gone.has(pages[i].id))
          trash.push({
            ...pages.splice(i, 1)[0],
            deleted_at: '2026-10-22T10:00:00Z',
          });
      return route.fulfill({ json: { data: null } });
    }
    if (method === 'GET' && path === 'pages/trash')
      return route.fulfill({ json: { data: trash } });
    if (method === 'GET' && /^pages(\/tree)?$/.test(path))
      return route.fulfill({ json: { data: pages.map((p) => ({ ...p })) } });
    return route.fulfill({ json: { data: [] } });
  });
  const deletes = () => calls.filter((c) => c.startsWith('DELETE'));
  return { pages, calls, deletes, errors };
}

const section = (page) => page.getByTestId('pages-sidebar-section');
const dialog = (page) =>
  page.getByRole('alertdialog', { name: 'Archive page' });

const openDelete = async (page, id, scope = section(page)) => {
  const rowEl = scope.locator(`[data-page-id="${id}"]`);
  await expect(rowEl).toBeVisible({ timeout: 10000 });
  await rowEl.hover();
  await rowEl.getByRole('button', { name: 'Page actions' }).click();
  await page.getByRole('menuitem', { name: 'Archive', exact: true }).click();
  await expect(dialog(page)).toBeVisible();
  // Wait for the zoom-in to settle before measuring or shooting.
  await expect
    .poll(() => dialog(page).evaluate((el) => el.getAnimations().length))
    .toBe(0);
};

const rgb = (color) =>
  color
    .match(/\d+(\.\d+)?/g)
    .slice(0, 3)
    .map(Number);

const expectReddish = async (button) => {
  const color = await button.evaluate(
    (el) => getComputedStyle(el).backgroundColor,
  );
  const [r, g, b] = rgb(color);
  expect(r, `confirm background ${color}`).toBeGreaterThan(g + 40);
  expect(r, `confirm background ${color}`).toBeGreaterThan(b + 40);
};

const expectBodyInteractive = async (page) => {
  await expect
    .poll(() =>
      page.evaluate(() => getComputedStyle(document.body).pointerEvents),
    )
    .not.toBe('none');
};

const shot = async (page, name) => {
  if (shotsDir)
    await page.screenshot({ path: `${shotsDir}/delete-dialog-${name}.png` });
};

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
});

for (const scheme of ['light', 'dark'])
  test(`the dialog shows the new copy and a destructive confirm (${scheme})`, async ({
    page,
  }) => {
    const api = await mockPages(page, { scheme });
    await page.goto('/demo/board');
    await expect(page.locator('html')).toHaveClass(
      scheme === 'dark' ? /\bdark\b/ : /^(?!.*\bdark\b)/,
    );
    await openDelete(page, 3);

    const box = dialog(page);
    await expect(box).toHaveCount(1);
    await expect(
      box.getByRole('heading', { name: 'Archive page' }),
    ).toBeVisible();
    await expect(box).toContainText(
      '"Roadmap" and all its subpages will be moved to the archive. You can restore them from there.',
    );
    await expect(box.getByRole('link', { name: 'archive' })).toBeVisible();
    await expect(box.getByRole('button', { name: 'Cancel' })).toBeFocused();
    const confirm = box.getByRole('button', { name: 'Archive', exact: true });
    await expect(confirm).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'Ok', exact: true }),
    ).toHaveCount(0);
    await expectReddish(confirm);
    await shot(page, scheme);

    await page.keyboard.press('Escape');
    await expect(box).toHaveCount(0);
    expect(api.deletes()).toEqual([]);
  });

test('Archive moves the page with its subpages out of the tree', async ({
  page,
}) => {
  const api = await mockPages(page);
  await page.goto('/demo/board');
  await openDelete(page, 1);
  await dialog(page)
    .getByRole('button', { name: 'Archive', exact: true })
    .click();

  await expect(dialog(page)).toHaveCount(0);
  await expect.poll(() => api.deletes()).toEqual(['DELETE pages/1']);
  await expect(section(page).locator('[data-page-id="1"]')).toHaveCount(0);
  await expect(section(page).locator('[data-page-id="2"]')).toHaveCount(0);
  await expect(section(page).locator('[data-page-id="3"]')).toBeVisible();

  await expectBodyInteractive(page);
  await section(page).locator('[data-page-id="3"] a').click();
  await expect(page).toHaveURL(/\/demo\/pages\/page-3$/);
  expect(api.errors).toEqual([]);
});

test('Cancel closes the dialog without deleting and leaves the tree clickable', async ({
  page,
}) => {
  const api = await mockPages(page);
  await page.goto('/demo/board');
  await openDelete(page, 3);
  await dialog(page).getByRole('button', { name: 'Cancel' }).click();

  await expect(dialog(page)).toHaveCount(0);
  await expect(section(page).locator('[data-page-id="3"]')).toBeVisible();
  await expectBodyInteractive(page);
  await section(page).locator('[data-page-id="3"] a').click();
  await expect(page).toHaveURL(/\/demo\/pages\/page-3$/);
  expect(api.deletes()).toEqual([]);
  expect(api.errors).toEqual([]);
});

test('Escape closes only the dialog', async ({ page }) => {
  const api = await mockPages(page);
  await page.goto('/demo/board');
  await openDelete(page, 3);
  const url = page.url();
  await page.keyboard.press('Escape');

  await expect(dialog(page)).toHaveCount(0);
  expect(page.url()).toBe(url);
  await expect(section(page)).toBeVisible();
  await expect(section(page).locator('[data-page-id="3"]')).toBeVisible();
  await expectBodyInteractive(page);
  expect(api.deletes()).toEqual([]);
});

test('the archive link opens the archive and closes the dialog', async ({
  page,
}) => {
  const api = await mockPages(page);
  await page.goto('/demo/board');
  await openDelete(page, 3);
  await dialog(page).getByRole('link', { name: 'archive' }).click();

  await expect(page).toHaveURL(/\/demo\/pages\/_\/archive$/);
  await expect(dialog(page)).toHaveCount(0);
  await expect(
    page.getByRole('heading', { name: 'Archive', exact: true }),
  ).toBeVisible();
  await expectBodyInteractive(page);
  expect(api.deletes()).toEqual([]);
});

test('a long unbroken title wraps inside a compact dialog', async ({
  page,
}) => {
  await mockPages(page);
  await page.goto('/demo/board');
  await openDelete(page, 4);
  await expect(dialog(page)).toContainText(LONG_TITLE);

  const size = await dialog(page).evaluate((el) => {
    const description = el.querySelector('p');
    return {
      width: el.getBoundingClientRect().width,
      scroll: el.scrollWidth,
      client: el.clientWidth,
      descScroll: description.scrollWidth,
      descClient: description.clientWidth,
    };
  });
  expect(size.width).toBeLessThanOrEqual(520);
  expect(size.scroll).toBeLessThanOrEqual(size.client);
  expect(size.descScroll).toBeLessThanOrEqual(size.descClient);
  await shot(page, 'long-title');
});

test('a rejected DELETE closes the dialog, keeps the row and shows a toast', async ({
  page,
}) => {
  const api = await mockPages(page, { deleteStatus: 422 });
  await page.goto('/demo/board');
  await openDelete(page, 3);
  await dialog(page)
    .getByRole('button', { name: 'Archive', exact: true })
    .click();

  await expect.poll(() => api.deletes()).toEqual(['DELETE pages/3']);
  await expect(dialog(page)).toHaveCount(0);
  await expect(
    page.getByText('Failed to archive page', { exact: true }),
  ).toBeVisible();
  await expect(section(page).locator('[data-page-id="3"]')).toBeVisible();
  await expectBodyInteractive(page);
  await section(page).locator('[data-page-id="3"] a').click();
  await expect(page).toHaveURL(/\/demo\/pages\/page-3$/);
  expect(api.errors).toEqual([]);
});

test('deleting from the All pages tree opens one dialog and updates both surfaces', async ({
  page,
}) => {
  const api = await mockPages(page);
  await page.goto('/demo/pages');
  const tree = page.getByTestId('pages-index-tree');
  await openDelete(page, 3, tree);
  await expect(dialog(page)).toHaveCount(1);
  await dialog(page)
    .getByRole('button', { name: 'Archive', exact: true })
    .click();

  await expect(dialog(page)).toHaveCount(0);
  await expect.poll(() => api.deletes()).toEqual(['DELETE pages/3']);
  await expect(tree.locator('[data-page-id="3"]')).toHaveCount(0);
  await expect(section(page).locator('[data-page-id="3"]')).toHaveCount(0);
  await expect(page).toHaveURL(/\/demo\/pages$/);
  expect(api.errors).toEqual([]);
});

test('a double click on Archive sends one DELETE', async ({ page }) => {
  const api = await mockPages(page);
  await page.goto('/demo/board');
  await openDelete(page, 3);
  await dialog(page)
    .getByRole('button', { name: 'Archive', exact: true })
    .dblclick();

  await expect(section(page).locator('[data-page-id="3"]')).toHaveCount(0);
  await page.waitForTimeout(500);
  expect(api.deletes()).toEqual(['DELETE pages/3']);
  expect(api.errors).toEqual([]);
});

test.describe('touch phone', () => {
  test.use({ hasTouch: true });

  test('Escape closes the dialog over the sidebar sheet, confirm keeps the sheet usable', async ({
    page,
  }) => {
    const api = await mockPages(page);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/demo/board');
    await page.waitForLoadState('networkidle');
    await page.locator('[data-tour="nav.menu-button"]').first().tap();
    const pick = async (id) => {
      const rowEl = section(page).locator(`[data-page-id="${id}"]`);
      await expect(rowEl).toBeVisible();
      await rowEl.getByRole('button', { name: 'Page actions' }).tap();
      await page.getByRole('menuitem', { name: 'Archive', exact: true }).tap();
      await expect(dialog(page)).toBeVisible();
      await expect(page.getByRole('menu')).toHaveCount(0);
      await expect
        .poll(() => dialog(page).evaluate((el) => getComputedStyle(el).opacity))
        .toBe('1');
    };

    await pick(3);
    await shot(page, 'phone');
    await page.keyboard.press('Escape');
    await expect(dialog(page)).toHaveCount(0);
    await expect(section(page)).toBeVisible();
    expect(api.deletes()).toEqual([]);

    await pick(3);
    await dialog(page)
      .getByRole('button', { name: 'Archive', exact: true })
      .tap();
    await expect(dialog(page)).toHaveCount(0);
    await expect(section(page).locator('[data-page-id="3"]')).toHaveCount(0);
    await expect(section(page)).toBeVisible();
    await expectBodyInteractive(page);
    await section(page).locator('[data-page-id="1"] a').tap();
    await expect(page).toHaveURL(/\/demo\/pages\/page-1$/);
    expect(api.deletes()).toEqual(['DELETE pages/3']);
    expect(api.errors).toEqual([]);
  });
});
