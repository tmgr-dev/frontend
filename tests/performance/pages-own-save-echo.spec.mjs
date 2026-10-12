import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const author = { kind: 'user', id: 1, name: 'Test User' };

const notes = (version, title = 'Notes') => ({
  id: 3,
  title,
  slug: 'notes',
  type: 'plain',
  parent_id: null,
  position: 1,
  pinned: false,
  workspace_id: 1,
  body: 'Hello',
  properties: {},
  version,
  author,
  updated_by: author,
  created_at: '2026-10-01T10:00:00Z',
  updated_at: '2026-10-01T10:00:00Z',
  backlinks: [],
  sections: [],
  files: [],
  following: false,
});

const fakePage = async (page, { hold = false } = {}) => {
  let current = notes(3);
  let releasePatch;
  const patchHeld = new Promise((resolve) => (releasePatch = resolve));
  const patches = [];
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
  await page.route('**/api/pages**', async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname.replace('/api/pages', '').replace(/^\//, '');
    const json = (data) => route.fulfill({ json: { data } });
    if (path === 'tree' || path === '') return json([notes(current.version)]);
    if ((path === 'notes' || path === '3') && request.method() === 'PATCH') {
      const body = request.postDataJSON();
      patches.push(body);
      if (body.version !== current.version)
        return route.fulfill({ status: 409, json: { error: 'page_conflict', data: current } });
      if (hold) await patchHeld;
      current = { ...current, ...request.postDataJSON(), version: current.version + 1 };
      return json(current);
    }
    if (path === 'notes' || path === '3') return json(current);
    return json([]);
  });
  return {
    patches,
    releasePatch: () => releasePatch(),
    serverChange: (version, title) => (current = notes(version, title)),
  };
};

const pageUpdated = (page, version) =>
  page.evaluate(
    (v) =>
      document
        .querySelector('#app')
        .__vue_app__.config.globalProperties.$store.commit('pagesEvent', {
          type: 'page.updated',
          page: { id: 3, version: v },
        }),
    version,
  );

test('the echo of my own save arriving before the PATCH response is not a change elsewhere', async ({
  page,
}) => {
  await mockApp(page);
  await page.addInitScript(() => sessionStorage.setItem('tmgr:tours:auto', 'true'));
  const server = await fakePage(page, { hold: true });
  await page.goto('/demo/pages/notes');
  const title = page.getByTestId('page-title-input');
  await expect(title).toHaveValue('Notes');

  await title.fill('Notes edited');
  await title.press('Control+s');
  await expect.poll(() => server.patches.length).toBe(1);

  await pageUpdated(page, 4);
  await page.waitForTimeout(300);
  server.releasePatch();
  await page.waitForTimeout(1000);
  await expect(page.getByTestId('page-update-banner')).toHaveCount(0);
  await expect(page.getByText('Not saved: version conflict')).toHaveCount(0);

  await pageUpdated(page, 4);
  await page.waitForTimeout(500);
  await expect(page.getByTestId('page-update-banner')).toHaveCount(0);

  server.serverChange(5, 'Renamed elsewhere');
  await pageUpdated(page, 5);
  await expect(page.getByTestId('page-update-banner')).toContainText('Page changed, version 5');
  await page.getByTestId('page-update-banner').getByRole('button', { name: 'Refresh' }).click();
  await expect(title).toHaveValue('Renamed elsewhere');
});

test('a change elsewhere while I have unsaved edits opens the conflict', async ({ page }) => {
  await mockApp(page);
  await page.addInitScript(() => sessionStorage.setItem('tmgr:tours:auto', 'true'));
  const server = await fakePage(page);
  await page.goto('/demo/pages/notes');
  const title = page.getByTestId('page-title-input');
  await expect(title).toHaveValue('Notes');

  server.serverChange(4, 'Renamed elsewhere');
  await pageUpdated(page, 4);
  await expect(page.getByTestId('page-update-banner')).toContainText('Page changed, version 4');
  await title.fill('My unsaved title');
  // Refresh (refreshAction) or the autosave's 409, whichever runs first, must end in the conflict.
  await page.getByTestId('page-update-banner').getByRole('button', { name: 'Refresh' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(title).toHaveValue('My unsaved title');
});
