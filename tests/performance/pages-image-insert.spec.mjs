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

const makePng = (width, height, pixelsPerMeter = null) => {
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
    ...(pixelsPerMeter
      ? [
          pngChunk(
            'pHYs',
            Buffer.from([
              ...[24, 16, 8, 0].map((s) => (pixelsPerMeter >>> s) & 255),
              ...[24, 16, 8, 0].map((s) => (pixelsPerMeter >>> s) & 255),
              1,
            ]),
          ),
        ]
      : []),
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

const gate = () => {
  let release;
  const wait = new Promise((resolve) => (release = resolve));
  return { wait, release };
};

// Registered after setupPages so these routes win over the pages catch-all.
const mockUpload = async (page, { failing = [], hold } = {}) => {
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
  await page.route('**/api/pages/1/files', async (route) => {
    if (route.request().method() !== 'POST')
      return route.fulfill({ json: { data: [] } });
    const body = route.request().postDataJSON();
    attaches.push(body.file_name);
    if (hold) await hold.wait;
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

const paragraph = (page, text) =>
  page.locator('.page-editor .ProseMirror p', { hasText: text });

const selectIn = async (line, collapse) => {
  await line.click();
  await line.evaluate((el, collapse) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    if (collapse) range.collapse(false);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
  }, collapse);
  await line.page().waitForTimeout(100);
};

const placeholder = (page) => page.locator('.page-editor .pe-upload-ghost');

const IMG = (id, size = '240x120') => `![](tmgr://file/${id} " =${size}")`;

const images = (page) => page.locator('.page-editor img.pe-image-img');

// Caret at the end of the first line, then Enter + the given slash query.
const typeSlash = async (page, query = '/') => {
  await selectIn(paragraph(page, 'First line'), true);
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
  await expect(images(page)).toHaveAttribute('src', /^blob:/);
  await expect
    .poll(async () => (await savedBody(state)).includes(IMG(41)))
    .toBe(true);
  expect(await savedBody(state)).toBe(
    `First line\n\n${IMG(41)}\n\nLast line\n`,
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
  await expect(images(page)).toBeVisible();
  await expect(page.locator('.page-editor .ProseMirror')).not.toContainText(
    '/ima',
  );
  await expect
    .poll(async () => (await savedBody(state)).includes(IMG(41)))
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
  await expect(images(page)).toHaveCount(2);
  await expect
    .poll(async () => {
      const body = await savedBody(state);
      return (
        body.indexOf(IMG(41)) >= 0 && body.indexOf(IMG(41)) < body.indexOf(IMG(42))
      );
    })
    .toBe(true);
});

test('upload 500 inserts nothing and shows Failed to upload', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n');
  const hold = gate();
  const attaches = await mockUpload(page, { failing: ['shot.png'], hold });
  await openPage(page);
  await typeSlash(page);
  const chooser = await chooseImage(page, () => imageItem(page).click());
  await chooser.setFiles(png('shot.png'));
  await expect(placeholder(page)).toBeVisible();
  hold.release();
  await expect(page.getByText('Failed to upload shot.png')).toBeVisible();
  await expect(placeholder(page)).toHaveCount(0);
  await expect(images(page)).toHaveCount(0);
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
  await expect(images(page)).toHaveCount(1);
  await expect(placeholder(page)).toHaveCount(0);
  await expect
    .poll(async () => (await savedBody(state)).includes(IMG(42)))
    .toBe(true);
  expect(await savedBody(state)).not.toContain('tmgr://file/41');
});

test('a pending upload lands where Image was chosen while the user edits elsewhere', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n\nLast line\n');
  const hold = gate();
  const attaches = await mockUpload(page, { hold });
  await openPage(page);
  await typeSlash(page);
  const chooser = await chooseImage(page, () => imageItem(page).click());
  await chooser.setFiles(png('shot.png'));
  await expect(placeholder(page)).toBeVisible();
  await expect.poll(() => attaches).toEqual(['shot.png']);
  const last = paragraph(page, 'Last line');
  await selectIn(last, true);
  await page.keyboard.type(' edited');
  await selectIn(last, false);
  hold.release();
  await expect(images(page)).toBeVisible();
  await expect(placeholder(page)).toHaveCount(0);
  await expect
    .poll(async () => (await savedBody(state)).includes(IMG(41)))
    .toBe(true);
  expect(await savedBody(state)).toBe(
    `First line\n\n${IMG(41)}\n\nLast line edited\n`,
  );
  expect(
    await page.evaluate(() => window.getSelection().toString()),
    'the selection made meanwhile is untouched',
  ).toBe('Last line edited');
});

test('text typed at the Image spot while uploading is kept next to the image', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n\nLast line\n');
  const hold = gate();
  await mockUpload(page, { hold });
  await openPage(page);
  await typeSlash(page);
  const chooser = await chooseImage(page, () => imageItem(page).click());
  await chooser.setFiles(png('shot.png'));
  await expect(placeholder(page)).toBeVisible();
  await page.keyboard.type('abc');
  await expect(paragraph(page, 'abc')).toBeVisible();
  hold.release();
  await expect(images(page)).toBeVisible();
  await expect(placeholder(page)).toHaveCount(0);
  await expect
    .poll(async () => (await savedBody(state)).includes(IMG(41)))
    .toBe(true);
  expect(await savedBody(state)).toBe(
    `First line\n\nabc${IMG(41)}\n\nLast line\n`,
  );
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
    await expect(images(page)).toHaveCount(0);
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
    .poll(async () => (await savedBody(state)).includes(IMG(41)))
    .toBe(true);
  expect(await savedBody(state)).toBe(`First line\n\n${IMG(41)}\n`);
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

const fileEvent = async (target, type, files, point, html) =>
  target.evaluate(
    (el, { type, files, point, html }) => {
      const transfer = new DataTransfer();
      if (html) transfer.setData('text/html', html);
      for (const f of files)
        transfer.items.add(
          new File([new Uint8Array(f.bytes)], f.name, { type: 'image/png' }),
        );
      const event =
        type === 'paste'
          ? new ClipboardEvent('paste', {
              clipboardData: transfer,
              bubbles: true,
              cancelable: true,
            })
          : new DragEvent(type, {
              dataTransfer: transfer,
              clientX: point.x,
              clientY: point.y,
              bubbles: true,
              cancelable: true,
            });
      el.dispatchEvent(event);
    },
    {
      type,
      point,
      html,
      files: files.map((f) => ({ name: f.name, bytes: [...f.buffer] })),
    },
  );

test('pasting an image into an empty line shows a ghost preview without the hint over it', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n');
  const hold = gate();
  await mockUpload(page, { hold });
  await openPage(page);
  await selectIn(paragraph(page, 'First line'), true);
  await page.keyboard.press('Enter');
  const empty = page.locator('.page-editor .ProseMirror p.crepe-placeholder');
  await expect(empty).toHaveCount(1);
  await fileEvent(page.locator('.page-editor .ProseMirror'), 'paste', [
    png('shot.png'),
  ]);
  const ghost = placeholder(page);
  await expect(ghost).toBeVisible();
  await expect(ghost.locator('img')).toHaveAttribute('src', /^blob:/);
  expect(
    await ghost.locator('img').evaluate((el) => getComputedStyle(el).opacity),
  ).toBe('0.5');
  await expect(page.locator('.page-editor')).not.toContainText(
    'Upload in progress',
  );
  const hint = await page
    .locator('.page-editor .ProseMirror p', {
      has: page.locator('.pe-upload-ghost'),
    })
    .evaluate((el) => getComputedStyle(el, '::before').content);
  expect(hint === 'none' || hint === 'normal').toBe(true);
  hold.release();
  await expect(images(page)).toHaveCount(1);
  await expect(placeholder(page)).toHaveCount(0);
  await expect
    .poll(async () => await savedBody(state))
    .toBe(`First line\n\n${IMG(41)}\n`);
});

test('the slash ghost sits in the emptied line and hides its hint', async ({
  page,
}) => {
  await setupPages(page, 'First line\n');
  const hold = gate();
  await mockUpload(page, { hold });
  await openPage(page);
  await typeSlash(page);
  const chooser = await chooseImage(page, () => imageItem(page).click());
  await chooser.setFiles(png('shot.png'));
  await expect(placeholder(page)).toBeVisible();
  const line = page.locator('.page-editor .ProseMirror p', {
    has: page.locator('.pe-upload-ghost'),
  });
  await expect(line).toHaveCount(1);
  const hint = await line.evaluate(
    (el) => getComputedStyle(el, '::before').content,
  );
  expect(hint === 'none' || hint === 'normal').toBe(true);
  hold.release();
  await expect(images(page)).toHaveCount(1);
});

test('dropping an image inserts it where it was dropped', async ({ page }) => {
  const state = await setupPages(page, 'First line\n\nLast line\n');
  await mockUpload(page);
  await openPage(page);
  const last = paragraph(page, 'Last line');
  const box = await last.boundingBox();
  await fileEvent(
    page.locator('.page-editor .ProseMirror'),
    'drop',
    [png('shot.png')],
    { x: box.x + 1, y: box.y + box.height / 2 },
  );
  await expect(images(page)).toHaveCount(1);
  await expect
    .poll(async () => await savedBody(state))
    .toBe(`First line\n\n${IMG(41)}Last line\n`);
});

test('a retina screenshot is stored at half its pixel size', async ({ page }) => {
  const state = await setupPages(page, 'First line\n');
  await mockUpload(page);
  await openPage(page);
  await typeSlash(page);
  const chooser = await chooseImage(page, () => imageItem(page).click());
  await chooser.setFiles({
    name: 'retina.png',
    mimeType: 'image/png',
    buffer: makePng(240, 120, 5669),
  });
  await expect
    .poll(async () => (await savedBody(state)).includes(IMG(41, '120x60')))
    .toBe(true);
});

test('pasting onto a selected image replaces it and keeps its width', async ({
  page,
}) => {
  const state = await setupPages(
    page,
    'First line\n\n![Cap](tmgr://file/7 "right-50 =100x50")\n',
  );
  const attaches = await mockUpload(page);
  await openPage(page);
  await images(page).first().click({ force: true });
  await expect(
    page.locator('.page-editor .pe-image.ProseMirror-selectednode'),
  ).toHaveCount(1);
  await fileEvent(page.locator('.page-editor .ProseMirror'), 'paste', [
    { name: 'new.png', mimeType: 'image/png', buffer: makePng(200, 200) },
  ]);
  await expect.poll(() => attaches).toEqual(['new.png']);
  await expect
    .poll(async () => await savedBody(state))
    .toBe('First line\n\n![Cap](tmgr://file/41 "right-50 =100x100")\n');
});

test('opening a page with existing images saves nothing', async ({ page }) => {
  const body =
    'Intro\n\n![shot.png](tmgr://file/1)\n\n![](tmgr://file/2)\n\n- item ![x](tmgr://file/3 " =50x20")\n\n| a | b |\n| - | - |\n| ![](tmgr://file/4) | c |\n\n![Cap](https://example.com/a.png "right-50 =300x200")\n';
  const state = await setupPages(page, body);
  await mockUpload(page);
  await openPage(page);
  await expect(images(page)).toHaveCount(5);
  await page.waitForTimeout(DEBOUNCE_MARGIN);
  expect(state.writes).toEqual([]);
  await expect(page.locator('.page-editor .pe-image-caption', { hasText: 'shot.png' })).toHaveCount(0);
});

test('aligning an image while its replacement uploads still swaps it', async ({
  page,
}) => {
  const state = await setupPages(
    page,
    'First line\n\n![Cap](tmgr://file/7 "right-50 =100x50")\n',
  );
  const hold = gate();
  const attaches = await mockUpload(page, { hold });
  await openPage(page);
  await images(page).first().click({ force: true });
  await fileEvent(page.locator('.page-editor .ProseMirror'), 'paste', [
    { name: 'new.png', mimeType: 'image/png', buffer: makePng(200, 200) },
  ]);
  await expect.poll(() => attaches).toEqual(['new.png']);
  await page
    .locator('.page-editor .pe-image-toolbar [data-action="align-center"]')
    .click();
  await expect
    .poll(async () => await savedBody(state))
    .toBe('First line\n\n![Cap](tmgr://file/7 " =100x50")\n');
  hold.release();
  await expect
    .poll(async () => await savedBody(state))
    .toBe('First line\n\n![Cap](tmgr://file/41 " =100x100")\n');
});

test('deleting an image while its replacement uploads leaves its copies alone', async ({
  page,
}) => {
  const copy = '![Cap](tmgr://file/7 " =100x50")';
  const state = await setupPages(page, `First line\n\n${copy}\n\n${copy}\n`);
  const hold = gate();
  const attaches = await mockUpload(page, { hold });
  await openPage(page);
  await images(page).first().click({ force: true });
  await fileEvent(page.locator('.page-editor .ProseMirror'), 'paste', [
    { name: 'new.png', mimeType: 'image/png', buffer: makePng(200, 200) },
  ]);
  await expect.poll(() => attaches).toEqual(['new.png']);
  await page
    .locator('.page-editor .pe-image-toolbar [data-action="delete"]')
    .first()
    .click();
  await expect(images(page)).toHaveCount(1);
  hold.release();
  await page.waitForTimeout(DEBOUNCE_MARGIN);
  const body = await savedBody(state);
  expect(body).not.toContain('tmgr://file/41');
  expect(body.split(copy)).toHaveLength(2);
});

test('pasting a snippet with text around an image keeps the text', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n');
  const attaches = await mockUpload(page);
  await openPage(page);
  await selectIn(paragraph(page, 'First line'), true);
  await fileEvent(
    page.locator('.page-editor .ProseMirror'),
    'paste',
    [png('shot.png')],
    undefined,
    '<p>Hello <b>world</b> <img src="https://example.com/x.png"></p>',
  );
  await expect
    .poll(async () => (await savedBody(state)).includes('Hello **world**'))
    .toBe(true);
  expect(attaches).toEqual([]);
});

const selectText = async (page, text) => {
  await selectIn(paragraph(page, text), false);
  expect(await page.evaluate(() => window.getSelection().toString())).toBe(text);
};

test('pasting an image over selected text keeps the text when the upload fails', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n\nReplace me\n');
  await mockUpload(page, { failing: ['shot.png'] });
  await openPage(page);
  await selectText(page, 'Replace me');
  await fileEvent(page.locator('.page-editor .ProseMirror'), 'paste', [
    png('shot.png'),
  ]);
  await expect(page.getByText('Failed to upload shot.png')).toBeVisible();
  await expect(placeholder(page)).toHaveCount(0);
  await expect(paragraph(page, 'Replace me')).toHaveCount(1);
  await page.waitForTimeout(DEBOUNCE_MARGIN);
  for (const write of state.writes)
    expect(write.body?.body ?? '').toContain('Replace me');
});

test('pasting an image over selected text replaces it once uploaded', async ({
  page,
}) => {
  const state = await setupPages(page, 'First line\n\nReplace me\n');
  const hold = gate();
  await mockUpload(page, { hold });
  await openPage(page);
  await selectText(page, 'Replace me');
  await fileEvent(page.locator('.page-editor .ProseMirror'), 'paste', [
    png('shot.png'),
  ]);
  await expect(placeholder(page)).toBeVisible();
  await expect(paragraph(page, 'Replace me')).toHaveCount(1);
  hold.release();
  await expect
    .poll(async () => await savedBody(state))
    .toBe(`First line\n\n${IMG(41)}\n`);
});
