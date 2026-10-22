import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const shotsDir = process.env.PAGES_ARCHIVE_SHOTS_DIR;

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

// Stateful like the backend: archive moves a subtree out of the tree with one
// deleted_at, GET pages/trash lists only archived roots, permanent delete and
// empty drop them for good, restore brings the subtree back.
async function mockPages(
  page,
  {
    archived = [],
    purgeStatus = 200,
    purgeBody,
    emptyStatus = 200,
    emptyBody = { message: 'Server error' },
    keep = [],
    scheme = 'light',
  } = {},
) {
  const pages = [
    row(1, 'Handbook'),
    row(2, 'Onboarding', 1),
    row(3, 'Roadmap'),
  ];
  const archive = archived.map((p, i) => ({
    deleted_at: `2026-10-2${i}T09:00:00Z`,
    ...p,
  }));
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
    const roots = () =>
      archive.filter(
        (p) =>
          !archive.some(
            (q) => q.id === p.parent_id && q.deleted_at === p.deleted_at,
          ),
      );
    const subtree = (id) => {
      const root = archive.find((p) => p.id === id);
      const ids = new Set([id]);
      for (let grew = true; grew; ) {
        grew = false;
        for (const p of archive)
          if (
            ids.has(p.parent_id) &&
            !ids.has(p.id) &&
            p.deleted_at === root.deleted_at
          ) {
            ids.add(p.id);
            grew = true;
          }
      }
      return ids;
    };
    const drop = (ids) => {
      for (let i = archive.length - 1; i >= 0; i--)
        if (ids.has(archive[i].id)) archive.splice(i, 1);
    };

    const archiveMatch = path.match(/^pages\/(\d+)$/);
    if (method === 'DELETE' && archiveMatch) {
      const gone = new Set([Number(archiveMatch[1])]);
      for (let grew = true; grew; ) {
        grew = false;
        for (const p of pages)
          if (gone.has(p.parent_id) && !gone.has(p.id)) {
            gone.add(p.id);
            grew = true;
          }
      }
      for (let i = pages.length - 1; i >= 0; i--)
        if (gone.has(pages[i].id))
          archive.push({
            ...pages.splice(i, 1)[0],
            deleted_at: '2026-10-22T10:00:00Z',
          });
      return route.fulfill({ json: { data: null } });
    }
    const purgeMatch = path.match(/^pages\/(\d+)\/permanent$/);
    if (method === 'DELETE' && purgeMatch) {
      const id = Number(purgeMatch[1]);
      if (purgeStatus === 404) drop(subtree(id));
      if (purgeStatus !== 200)
        return route.fulfill(
          purgeBody === undefined
            ? { status: purgeStatus, body: '' }
            : { status: purgeStatus, json: purgeBody },
        );
      const ids = subtree(id);
      drop(ids);
      return route.fulfill({ json: { data: { deleted: ids.size, files: 0 } } });
    }
    if (method === 'POST' && path === 'pages/trash/empty') {
      if (emptyStatus !== 200)
        return route.fulfill(
          emptyBody === null
            ? { status: emptyStatus, body: '' }
            : { status: emptyStatus, json: emptyBody },
        );
      const gone = roots().filter((p) => !keep.includes(p.id));
      const before = archive.length;
      for (const p of gone) drop(subtree(p.id));
      return route.fulfill({
        json: {
          data: {
            purged: gone.length,
            deleted: before - archive.length,
            skipped: roots().length,
          },
        },
      });
    }
    const restoreMatch = path.match(/^pages\/(\d+)\/restore$/);
    if (method === 'POST' && restoreMatch) {
      const ids = subtree(Number(restoreMatch[1]));
      for (let i = archive.length - 1; i >= 0; i--)
        if (ids.has(archive[i].id)) {
          const { deleted_at, ...live } = archive.splice(i, 1)[0];
          pages.push(live);
        }
      const restored = pages.find((p) => p.id === Number(restoreMatch[1]));
      return route.fulfill({ json: { data: restored } });
    }
    if (method === 'GET' && path === 'pages/trash')
      return route.fulfill({ json: { data: roots().map((p) => ({ ...p })) } });
    if (method === 'GET' && /^pages(\/tree)?$/.test(path))
      return route.fulfill({ json: { data: pages.map((p) => ({ ...p })) } });
    return route.fulfill({ json: { data: [] } });
  });
  const only = (prefix) => calls.filter((c) => c.startsWith(prefix));
  return {
    calls,
    errors,
    purges: () => calls.filter((c) => c.endsWith('/permanent')),
    empties: () => only('POST pages/trash/empty'),
    trashGets: () => only('GET pages/trash').length,
  };
}

const section = (page) => page.getByTestId('pages-sidebar-section');
const archiveRow = (page, id) =>
  page
    .locator(`li[data-page-id="${id}"]`)
    .filter({ has: page.getByRole('button', { name: 'Restore' }) });
const purgeDialog = (page) =>
  page.getByRole('alertdialog', { name: 'Delete permanently?' });
const emptyDialog = (page) =>
  page.getByRole('alertdialog', { name: 'Delete all archived pages?' });
const deleteAll = (page) => page.getByRole('button', { name: 'Delete all' });

const settle = async (dialog) =>
  expect
    .poll(() => dialog.evaluate((el) => getComputedStyle(el).transform))
    .toMatch(/^(none|matrix\(1, 0, 0, 1, .*)$/);

const shot = async (page, name) => {
  if (shotsDir)
    await page.screenshot({ path: `${shotsDir}/pages-archive-${name}.png` });
};

const openArchive = async (page) => {
  await page.goto('/demo/pages/_/archive');
  await expect(
    page.getByRole('heading', { name: 'Archive', exact: true }),
  ).toBeVisible();
};

const askPurge = async (page, id) => {
  await archiveRow(page, id)
    .getByRole('button', { name: 'Delete permanently' })
    .click();
  await expect(purgeDialog(page)).toBeVisible();
  await settle(purgeDialog(page));
};

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
});

test('a page archived from the tree is listed in Archive and deleted permanently from there', async ({
  page,
}) => {
  const api = await mockPages(page);
  await page.goto('/demo/board');

  const tree = section(page).locator('[data-page-id="1"]');
  await expect(tree).toBeVisible({ timeout: 10000 });
  await tree.hover();
  await tree.getByRole('button', { name: 'Page actions' }).click();
  await page.getByRole('menuitem', { name: 'Archive', exact: true }).click();
  await page
    .getByRole('alertdialog', { name: 'Archive page' })
    .getByRole('button', { name: 'Archive', exact: true })
    .click();
  await expect(tree).toHaveCount(0);

  const link = section(page).getByRole('link', { name: 'Archive' });
  await expect(link).toHaveAttribute('href', '/demo/pages/_/archive');
  await link.click();
  await expect(page).toHaveURL(/\/demo\/pages\/_\/archive$/);
  await expect(
    page.getByRole('heading', { name: 'Archive', exact: true }),
  ).toBeVisible();
  await expect(archiveRow(page, 1)).toContainText('Handbook');
  await expect(archiveRow(page, 1)).toContainText('Archived');
  await expect(archiveRow(page, 2)).toHaveCount(0);
  await expect(
    archiveRow(page, 1).getByRole('button', { name: 'Restore' }),
  ).toBeVisible();

  await askPurge(page, 1);
  const box = purgeDialog(page);
  await expect(box).toContainText(
    '"Handbook" and its archived subpages will be deleted forever. This can\'t be undone.',
  );
  await expect(box.getByRole('button', { name: 'Cancel' })).toBeVisible();
  await box.getByRole('button', { name: 'Cancel' }).click();
  await expect(box).toHaveCount(0);
  expect(api.purges()).toEqual([]);

  await askPurge(page, 1);
  await purgeDialog(page)
    .getByRole('button', { name: 'Delete permanently' })
    .dblclick();
  await expect(purgeDialog(page)).toHaveCount(0);
  await expect(archiveRow(page, 1)).toHaveCount(0);
  await expect(page.getByText('Archive is empty.')).toBeVisible();
  await expect(deleteAll(page)).toHaveCount(0);
  await page.waitForTimeout(300);
  expect(api.purges()).toEqual(['DELETE pages/1/permanent']);
  await expect(section(page).locator('[data-page-id="1"]')).toHaveCount(0);
  expect(api.errors).toEqual([]);
});

test('Restore brings the page back and opens it', async ({ page }) => {
  const api = await mockPages(page, { archived: [row(7, 'Old notes')] });
  await openArchive(page);
  await archiveRow(page, 7).getByRole('button', { name: 'Restore' }).click();

  await expect(page).toHaveURL(/\/demo\/pages\/page-7$/);
  expect(api.calls).toContain('POST pages/7/restore');
  await expect(section(page).locator('[data-page-id="7"]')).toBeVisible();
  expect(api.purges()).toEqual([]);
  expect(api.errors).toEqual([]);
});

test('Delete all needs the typed word and empties the archive', async ({
  page,
}) => {
  const api = await mockPages(page, {
    archived: [
      row(7, 'Old notes'),
      { ...row(8, 'Drafts'), deleted_at: '2026-10-21T09:00:00Z' },
      { ...row(9, 'Ideas', 8), deleted_at: '2026-10-21T09:00:00Z' },
    ],
  });
  await openArchive(page);
  await expect(archiveRow(page, 7)).toBeVisible();
  await expect(archiveRow(page, 9)).toHaveCount(0);

  await deleteAll(page).click();
  const box = emptyDialog(page);
  await expect(box).toBeVisible();
  await settle(box);
  await expect(box).toContainText(
    "All 2 archived pages and their subpages will be deleted forever. This can't be undone.",
  );
  const confirm = box.getByRole('button', { name: 'Delete all' });
  const input = box.getByRole('textbox');
  await expect(confirm).toBeDisabled();
  await input.fill('dele');
  await expect(confirm).toBeDisabled();
  await input.press('Enter');
  await expect(box).toBeVisible();
  await input.fill('deleted');
  await expect(confirm).toBeDisabled();
  await input.fill('  DELETE ');
  await expect(confirm).toBeEnabled();
  expect(api.empties()).toEqual([]);

  await confirm.dblclick();
  await expect(box).toHaveCount(0);
  await expect(page.getByText('Archive is empty.')).toBeVisible();
  await expect(deleteAll(page)).toHaveCount(0);
  await page.waitForTimeout(300);
  expect(api.empties()).toEqual(['POST pages/trash/empty']);
  expect(api.errors).toEqual([]);
});

test('Delete all opens with an empty field every time', async ({ page }) => {
  await mockPages(page, { archived: [row(7, 'Old notes')] });
  await openArchive(page);
  await deleteAll(page).click();
  await expect(emptyDialog(page)).toContainText(
    "The archived page and its subpages will be deleted forever. This can't be undone.",
  );
  await emptyDialog(page).getByRole('textbox').fill('delete');
  await emptyDialog(page).getByRole('button', { name: 'Cancel' }).click();
  await expect(emptyDialog(page)).toHaveCount(0);

  await deleteAll(page).click();
  await expect(emptyDialog(page).getByRole('textbox')).toHaveValue('');
  await expect(
    emptyDialog(page).getByRole('button', { name: 'Delete all' }),
  ).toBeDisabled();
});

test('an empty archive shows the empty state and no Delete all', async ({
  page,
}) => {
  const api = await mockPages(page);
  await openArchive(page);
  await expect(page.getByText('Archive is empty.')).toBeVisible();
  await expect(deleteAll(page)).toHaveCount(0);
  expect(api.errors).toEqual([]);
});

test('a 403 with an empty body shows a permission toast and reloads the archive', async ({
  page,
}) => {
  const api = await mockPages(page, {
    archived: [row(7, 'Old notes')],
    purgeStatus: 403,
  });
  await openArchive(page);
  await expect(archiveRow(page, 7)).toBeVisible();
  const before = api.trashGets();

  await askPurge(page, 7);
  await purgeDialog(page)
    .getByRole('button', { name: 'Delete permanently' })
    .click();

  await expect(purgeDialog(page)).toHaveCount(0);
  await expect(
    page.getByText("You don't have permission to delete this page", {
      exact: true,
    }),
  ).toBeVisible();
  await expect.poll(() => api.trashGets()).toBeGreaterThan(before);
  await expect(archiveRow(page, 7)).toBeVisible();
  expect(api.purges()).toEqual(['DELETE pages/7/permanent']);
  expect(api.errors).toEqual([]);
});

test('a 404 (already purged) reloads the archive and the row is gone', async ({
  page,
}) => {
  const api = await mockPages(page, {
    archived: [row(7, 'Old notes'), row(8, 'Drafts')],
    purgeStatus: 404,
    purgeBody: { message: 'Page not found', error: 'page_not_found' },
  });
  await openArchive(page);
  await askPurge(page, 7);
  await purgeDialog(page)
    .getByRole('button', { name: 'Delete permanently' })
    .click();

  await expect(archiveRow(page, 7)).toHaveCount(0);
  await expect(archiveRow(page, 8)).toBeVisible();
  expect(api.errors).toEqual([]);
});

test('a failed Delete all shows an error and reloads the archive', async ({
  page,
}) => {
  const api = await mockPages(page, {
    archived: [row(7, 'Old notes')],
    emptyStatus: 500,
  });
  await openArchive(page);
  const before = api.trashGets();
  await deleteAll(page).click();
  await emptyDialog(page).getByRole('textbox').fill('delete');
  await emptyDialog(page).getByRole('button', { name: 'Delete all' }).click();

  await expect(emptyDialog(page)).toHaveCount(0);
  await expect(
    page.getByText('Failed to empty the archive', { exact: true }),
  ).toBeVisible();
  await expect.poll(() => api.trashGets()).toBeGreaterThan(before);
  await expect(archiveRow(page, 7)).toBeVisible();
  expect(api.errors).toEqual([]);
});

test('a 403 with an empty body on Delete all shows a permission toast and reloads', async ({
  page,
}) => {
  const api = await mockPages(page, {
    archived: [row(7, 'Old notes')],
    emptyStatus: 403,
    emptyBody: null,
  });
  await openArchive(page);
  const before = api.trashGets();
  await deleteAll(page).click();
  await emptyDialog(page).getByRole('textbox').fill('delete');
  await emptyDialog(page).getByRole('button', { name: 'Delete all' }).click();

  await expect(emptyDialog(page)).toHaveCount(0);
  await expect(
    page.getByText("You don't have permission to empty the archive", {
      exact: true,
    }),
  ).toBeVisible();
  await expect.poll(() => api.trashGets()).toBeGreaterThan(before);
  await expect(archiveRow(page, 7)).toBeVisible();
  expect(api.errors).toEqual([]);
});

test('Delete all keeps the pages a member may not delete and says so', async ({
  page,
}) => {
  const api = await mockPages(page, {
    archived: [row(7, 'Old notes'), row(8, 'Owner page')],
    keep: [8],
  });
  await openArchive(page);
  await deleteAll(page).click();
  await emptyDialog(page).getByRole('textbox').fill('delete');
  await emptyDialog(page).getByRole('button', { name: 'Delete all' }).click();

  await expect(emptyDialog(page)).toHaveCount(0);
  await expect(
    page.getByText('Some pages were kept', { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("1 archived page you can't delete stayed in the archive.", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(archiveRow(page, 7)).toHaveCount(0);
  await expect(archiveRow(page, 8)).toBeVisible();
  await expect(deleteAll(page)).toBeVisible();
  expect(api.empties()).toEqual(['POST pages/trash/empty']);
  expect(api.errors).toEqual([]);
});

test('the old trash URL lands on the archive', async ({ page }) => {
  await mockPages(page);
  await page.goto('/demo/pages/_/trash');
  await expect(page).toHaveURL(/\/demo\/pages\/_\/archive$/);
  await expect(
    page.getByRole('heading', { name: 'Archive', exact: true }),
  ).toBeVisible();
});

test('All pages links to the archive', async ({ page }) => {
  await mockPages(page);
  await page.goto('/demo/pages');
  const link = page
    .locator('main')
    .getByRole('link', { name: 'Archive', exact: true });
  await expect(link).toHaveAttribute('href', '/demo/pages/_/archive');
  await link.click();
  await expect(page).toHaveURL(/\/demo\/pages\/_\/archive$/);
});

test('on a phone each archived row keeps its title readable', async ({
  page,
}) => {
  await mockPages(page, { archived: [row(7, 'Old notes')] });
  await page.setViewportSize({ width: 390, height: 844 });
  await openArchive(page);
  const item = archiveRow(page, 7);
  const title = item.getByText('Old notes', { exact: true });
  const restore = item.getByRole('button', { name: 'Restore' });
  const purge = item.getByRole('button', { name: 'Delete permanently' });
  const [t, r, d] = await Promise.all(
    [title, restore, purge].map((l) => l.boundingBox()),
  );
  expect(t.width, 'title width').toBeGreaterThanOrEqual(
    await title.evaluate((el) => el.scrollWidth),
  );
  const overlaps = (a, b) =>
    a.x < b.x + b.width &&
    b.x < a.x + a.width &&
    a.y < b.y + b.height &&
    b.y < a.y + a.height;
  expect(overlaps(t, r), 'title vs Restore').toBe(false);
  expect(overlaps(t, d), 'title vs Delete permanently').toBe(false);
  const subtitle = await item.getByText(/^Archived /).boundingBox();
  expect(overlaps(subtitle, r), 'date vs Restore').toBe(false);
});

for (const scheme of ['light', 'dark'])
  for (const [label, width, height] of [
    ['desktop', 1280, 800],
    ['phone', 390, 844],
  ])
    test(`archive page and dialogs fit (${label}, ${scheme})`, async ({
      page,
    }) => {
      const api = await mockPages(page, {
        scheme,
        archived: [
          row(7, 'Old notes'),
          row(8, 'Quarterly'.repeat(12)),
          row(9, 'Drafts'),
        ],
      });
      await page.setViewportSize({ width, height });
      await openArchive(page);
      await expect(archiveRow(page, 9)).toBeVisible();
      const overflow = () =>
        page.evaluate(
          () =>
            document.documentElement.scrollWidth -
            document.documentElement.clientWidth,
        );
      expect(await overflow()).toBeLessThanOrEqual(0);
      for (const id of [7, 8, 9]) {
        const buttons = archiveRow(page, id).getByRole('button');
        for (const b of await buttons.all()) await expect(b).toBeInViewport();
      }
      await shot(page, `list-${label}-${scheme}`);

      await askPurge(page, 8);
      await expect(purgeDialog(page)).toBeInViewport({ ratio: 1 });
      const fits = await purgeDialog(page).evaluate(
        (el) => el.scrollWidth <= el.clientWidth,
      );
      expect(fits).toBe(true);
      await shot(page, `purge-${label}-${scheme}`);
      await page.keyboard.press('Escape');
      await expect(purgeDialog(page)).toHaveCount(0);

      await deleteAll(page).click();
      await expect(emptyDialog(page)).toBeVisible();
      await settle(emptyDialog(page));
      await expect(emptyDialog(page)).toBeInViewport({ ratio: 1 });
      await shot(page, `empty-${label}-${scheme}`);
      await page.keyboard.press('Escape');
      await expect(emptyDialog(page)).toHaveCount(0);
      expect(api.purges()).toEqual([]);
      expect(api.empties()).toEqual([]);
      expect(api.errors).toEqual([]);
    });
