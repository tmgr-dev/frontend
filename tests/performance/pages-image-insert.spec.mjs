import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { crc32, deflateSync } from 'node:zlib';
import { mockApp } from './mockApp.mjs';

const shotDir = process.env.PAGES_SHOTS;
if (shotDir) mkdirSync(shotDir, { recursive: true });

const pngChunk = (type, data) => {
  const head = Buffer.alloc(4);
  head.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const tail = Buffer.alloc(4);
  tail.writeUInt32BE(crc32(body));
  return Buffer.concat([head, body, tail]);
};

const makePng = (width, height) => {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header.set([8, 2, 0, 0, 0], 8);
  const rows = [];
  for (let y = 0; y < height; y++) {
    const row = Buffer.alloc(1 + width * 3);
    for (let x = 0; x < width; x++) {
      const stripe = Math.floor(x / 20) % 2 === 0;
      row.set(stripe ? [59, 130, 246] : [147, 197, 253], 1 + x * 3);
    }
    rows.push(row);
  }
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk('IHDR', header),
    pngChunk('IDAT', deflateSync(Buffer.concat(rows))),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
};

const PNG = makePng(240, 120);
const png = (name) => ({ name, mimeType: 'image/png', buffer: PNG });

const author = { kind: 'user', id: 1, name: 'Test User' };

const setupPages = async (page, body) => {
  await mockApp(page);
  const state = {
    page: { id: 1, title: 'Rich page', slug: 'rich-page', body, version: 3 },
    writes: [],
  };
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
    version: p.version,
    author,
    updated_by: author,
    created_at: '2026-09-30T10:00:00Z',
    backlinks: [],
    sections: [],
    files: [],
    following: false,
  });

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
  await page.route('**/api/files/**', (route) => {
    const path = new URL(route.request().url()).pathname.replace(
      '/api/files/',
      '',
    );
    if (/^\d+\/content$/.test(path))
      return route.fulfill({ body: PNG, contentType: 'image/png' });
    if (/^\d+\/signed-url$/.test(path))
      return route.fulfill({ status: 503, json: {} });
    return route.fulfill({ json: { data: null } });
  });
  await page.route('**/api/pages**', (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname
      .replace('/api/pages', '')
      .replace(/^\//, '');
    const json = (data) => route.fulfill({ json: { data } });
    if (request.method() !== 'GET') {
      const body = request.postDataJSON();
      state.writes.push({ path, body });
      if (path !== '1') return json(null);
      if (typeof body?.body === 'string') state.page.body = body.body;
      state.page.version += 1;
      return json(full(state.page));
    }
    if (path === 'tree' || path === '') return json([summary(state.page)]);
    if (path === state.page.slug || path === '1') return json(full(state.page));
    return json([]);
  });
  await page.route('**/api/workspaces/1/categories*', (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  return state;
};

// Registered after setupPages so these routes win over the pages catch-all.
const mockUpload = async (page, { failing = [] } = {}) => {
  const attaches = [];
  await page.route('**/api/files/presign-upload', (route) => {
    const body = route.request().postDataJSON();
    return route.fulfill({
      json: {
        data: {
          key: `uploads/${body.file_name}`,
          upload_url: 'http://storage.test/storage-put',
          method: 'PUT',
          content_type: body.content_type || 'application/octet-stream',
          max_bytes: 10_000_000,
        },
      },
    });
  });
  await page.route('**/storage-put', (route) =>
    route.fulfill({ status: 200, body: '' }),
  );
  await page.route('**/api/pages/1/files', (route) => {
    if (route.request().method() !== 'POST')
      return route.fulfill({ json: { data: [] } });
    const body = route.request().postDataJSON();
    attaches.push(body.file_name);
    if (failing.includes(body.file_name))
      return route.fulfill({ status: 500, json: { message: 'Boom' } });
    return route.fulfill({
      status: 201,
      json: {
        data: {
          id: 40 + attaches.length,
          page_id: 1,
          name: body.file_name,
          mime_type: body.mime_type,
          size: body.size_bytes,
          created_at: '2026-10-01T10:00:00Z',
        },
      },
    });
  });
  return attaches;
};

const useScheme = async (page, scheme) => {
  await page.emulateMedia({ colorScheme: scheme });
  await page.addInitScript(
    (value) => localStorage.setItem('colorScheme', value),
    scheme,
  );
};

const savedBody = async (state) => {
  await expect
    .poll(() => state.writes.length, { timeout: 10_000 })
    .toBeGreaterThan(0);
  return state.writes.at(-1).body.body;
};

const DEBOUNCE_MARGIN = 2500;

const openPage = async (page) => {
  await page.goto('/demo/pages/rich-page');
  await expect(
    page.locator('[data-testid="page-content"] .page-editor .ProseMirror'),
  ).toHaveCount(1);
  await expect(page.getByTestId('page-save-status')).toHaveText(
    /^Saved, version/,
  );
};

const slashMenu = (page) =>
  page.locator('.page-editor .milkdown-slash-menu[data-show="true"]');

const imageItem = (page) =>
  slashMenu(page)
    .locator('.menu-group', { has: page.locator('h6', { hasText: 'Insert' }) })
    .locator('li', { hasText: /^Image$/ });

// Caret at the end of the first line, then Enter + the given slash query.
const typeSlash = async (page, query = '/') => {
  const line = page.locator('.page-editor .ProseMirror p', {
    hasText: 'First line',
  });
  await line.click();
  await line.evaluate((el) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
  });
  await page.waitForTimeout(100);
  await page.keyboard.press('Enter');
  await page.keyboard.type(query);
  await expect(slashMenu(page)).toBeVisible();
};

const chooseImage = async (page, run) => {
  const [chooser] = await Promise.all([
    page.waitForEvent('filechooser'),
    run(),
  ]);
  expect(chooser.isMultiple()).toBe(true);
  expect(await chooser.element().getAttribute('accept')).toBe('image/*');
  return chooser;
};

test.describe.configure({ timeout: 90_000 });

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
});

test('/ menu lists Image in the Insert group', async ({ page }) => {
  await setupPages(page, 'First line\n');
  await mockUpload(page);
  await openPage(page);
  await typeSlash(page);
  await expect(imageItem(page)).toBeVisible();
  await expect(imageItem(page).locator('svg')).toHaveCount(1);
});

test('clicking Image uploads the picked PNG and saves a tmgr://file image', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n\nLast line\n');
  const attaches = await mockUpload(page);
  await openPage(page);
  await typeSlash(page);
  const chooser = await chooseImage(page, () => imageItem(page).click());
  await expect(slashMenu(page)).toBeHidden();
  await chooser.setFiles(png('shot.png'));
  await expect.poll(() => attaches).toEqual(['shot.png']);
  await expect(
    page.locator('.page-editor img[alt="shot.png"]'),
  ).toHaveAttribute('src', /^blob:/);
  await expect
    .poll(async () =>
      (await savedBody(state)).includes('![shot.png](tmgr://file/41)'),
    )
    .toBe(true);
  expect(await savedBody(state)).toBe(
    'First line\n\n![shot.png](tmgr://file/41)\n\nLast line\n',
  );
  expect(await page.locator('body > input[type=file]').count()).toBe(0);
});

test('/ima + Enter picks Image with the keyboard and removes the query', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n');
  const attaches = await mockUpload(page);
  await openPage(page);
  await typeSlash(page, '/ima');
  await expect(slashMenu(page).locator('li.hover')).toHaveText('Image');
  const chooser = await chooseImage(page, () => page.keyboard.press('Enter'));
  await chooser.setFiles(png('shot.png'));
  await expect.poll(() => attaches).toEqual(['shot.png']);
  await expect(page.locator('.page-editor img[alt="shot.png"]')).toBeVisible();
  await expect(page.locator('.page-editor .ProseMirror')).not.toContainText(
    '/ima',
  );
  await expect
    .poll(async () =>
      (await savedBody(state)).includes('![shot.png](tmgr://file/41)'),
    )
    .toBe(true);
  expect(await savedBody(state)).not.toContain('/ima');
});

test('several picked images are all inserted in order', async ({ page }) => {
  const state = await setupPages(page, 'First line\n');
  const attaches = await mockUpload(page);
  await openPage(page);
  await typeSlash(page);
  const chooser = await chooseImage(page, () => imageItem(page).click());
  await chooser.setFiles([png('one.png'), png('two.png')]);
  await expect.poll(() => attaches).toEqual(['one.png', 'two.png']);
  await expect(page.locator('.page-editor img[alt$=".png"]')).toHaveCount(2);
  await expect
    .poll(async () => {
      const body = await savedBody(state);
      return (
        body.indexOf('![one.png](tmgr://file/41)') >= 0 &&
        body.indexOf('![one.png](tmgr://file/41)') <
          body.indexOf('![two.png](tmgr://file/42)')
      );
    })
    .toBe(true);
});

test('upload 500 inserts nothing and shows Failed to upload', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n');
  const attaches = await mockUpload(page, { failing: ['shot.png'] });
  await openPage(page);
  await typeSlash(page);
  const chooser = await chooseImage(page, () => imageItem(page).click());
  await chooser.setFiles(png('shot.png'));
  await expect(page.getByText('Failed to upload shot.png')).toBeVisible();
  await expect(page.locator('.page-editor img[alt$=".png"]')).toHaveCount(0);
  await page.waitForTimeout(DEBOUNCE_MARGIN);
  expect(attaches, 'a failed mutation is not replayed').toEqual(['shot.png']);
  for (const write of state.writes)
    expect(write.body?.body ?? '').not.toContain('tmgr://file');
});

test('one failed file of two still inserts the other', async ({ page }) => {
  const state = await setupPages(page, 'First line\n');
  await mockUpload(page, { failing: ['bad.png'] });
  await openPage(page);
  await typeSlash(page);
  const chooser = await chooseImage(page, () => imageItem(page).click());
  await chooser.setFiles([png('bad.png'), png('good.png')]);
  await expect(page.getByText('Failed to upload bad.png')).toBeVisible();
  await expect(page.locator('.page-editor img[alt="good.png"]')).toBeVisible();
  await expect
    .poll(async () => (await savedBody(state)).includes('![good.png]('))
    .toBe(true);
  expect(await savedBody(state)).not.toContain('bad.png');
});

for (const how of ['cancel event', 'empty selection']) {
  test(`cancelling the picker (${how}) leaves no image and no error`, async ({
    page,
  }) => {
    const errors = [];
    // The graph panel throws on this minimal page mock; unrelated to the editor.
    page.on('pageerror', (error) => {
      if (!error.stack?.includes('/components/graph/'))
        errors.push(error.stack);
    });
    const state = await setupPages(page, 'First line\n');
    const attaches = await mockUpload(page);
    await openPage(page);
    await typeSlash(page);
    const chooser = await chooseImage(page, () => imageItem(page).click());
    if (how === 'cancel event')
      await chooser
        .element()
        .evaluate((el) => el.dispatchEvent(new Event('cancel')));
    else await chooser.setFiles([]);
    await expect(page.locator('body > input[type=file]')).toHaveCount(0);
    await page.waitForTimeout(DEBOUNCE_MARGIN);
    await expect(page.locator('.page-editor img[alt$=".png"]')).toHaveCount(0);
    expect(attaches).toEqual([]);
    for (const write of state.writes) {
      expect(write.body?.body ?? '').not.toContain('tmgr://file');
      expect(write.body?.body ?? '').not.toContain('/');
    }
    expect(errors).toEqual([]);
    await page.keyboard.type('still typing');
    await expect
      .poll(async () => (await savedBody(state)).includes('still typing'))
      .toBe(true);
  });
}

test('the + block handle menu inserts an image and keeps the line', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n');
  await mockUpload(page);
  await openPage(page);
  const line = page.locator('.page-editor .ProseMirror p', {
    hasText: 'First line',
  });
  await line.hover();
  const handle = page.locator(
    '.page-editor .milkdown-block-handle[data-show="true"]',
  );
  await expect(handle).toBeVisible();
  await handle.locator('.operation-item').first().hover();
  await handle.locator('.operation-item').first().click();
  const chooser = await chooseImage(page, () => imageItem(page).click());
  await chooser.setFiles(png('shot.png'));
  await expect
    .poll(async () =>
      (await savedBody(state)).includes('![shot.png](tmgr://file/41)'),
    )
    .toBe(true);
  expect(await savedBody(state)).toBe(
    'First line\n\n![shot.png](tmgr://file/41)\n',
  );
});

for (const scheme of ['light', 'dark']) {
  test(`Image item is legible in ${scheme}`, async ({ page }) => {
    await useScheme(page, scheme);
    await setupPages(page, 'First line\n');
    await mockUpload(page);
    await openPage(page);
    await typeSlash(page);
    const item = imageItem(page);
    await expect(item).toBeVisible();
    const colors = await item.evaluate((el) => {
      const svg = el.querySelector('svg');
      const bg = (node) => {
        for (let n = node; n; n = n.parentElement) {
          const c = getComputedStyle(n).backgroundColor;
          if (c !== 'rgba(0, 0, 0, 0)' && c !== 'transparent') return c;
        }
        return '';
      };
      return {
        stroke: getComputedStyle(svg).color,
        text: getComputedStyle(el.querySelector('span')).color,
        bg: bg(el),
        dark: document.documentElement.classList.contains('dark'),
      };
    });
    expect(colors.dark).toBe(scheme === 'dark');
    expect(colors.stroke).not.toBe(colors.bg);
    expect(colors.text).not.toBe(colors.bg);
    if (shotDir) {
      await item.scrollIntoViewIfNeeded();
      await slashMenu(page).screenshot({
        path: `${shotDir}/image-item-${scheme}.png`,
      });
    }
  });
}
