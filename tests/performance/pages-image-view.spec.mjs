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

const PNG = makePng(400, 200);
const author = { kind: 'user', id: 1, name: 'Test User' };
const IMAGE = '![](tmgr://file/1 " =400x200")';
const DEBOUNCE_MARGIN = 2500;

const setupPages = async (page, body, { missing = [] } = {}) => {
  await mockApp(page);
  const state = {
    page: { id: 1, title: 'Rich page', slug: 'rich-page', body, version: 3 },
    writes: [],
    attaches: [],
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
    const id = Number(path.split('/')[0]);
    if (/^\d+\/content$/.test(path))
      return missing.includes(id)
        ? route.fulfill({ status: 404, json: {} })
        : route.fulfill({ body: PNG, contentType: 'image/png' });
    if (/^\d+\/signed-url$/.test(path))
      return route.fulfill({ status: 503, json: {} });
    if (path === 'presign-upload') {
      const request = route.request().postDataJSON();
      return route.fulfill({
        json: {
          data: {
            key: `uploads/${request.file_name}`,
            upload_url: 'http://storage.test/storage-put',
            method: 'PUT',
            content_type: request.content_type || 'application/octet-stream',
            max_bytes: 10_000_000,
          },
        },
      });
    }
    return route.fulfill({ json: { data: null } });
  });
  await page.route('**/storage-put', (route) =>
    route.fulfill({ status: 200, body: '' }),
  );
  await page.route('**/api/pages**', (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname
      .replace('/api/pages', '')
      .replace(/^\//, '');
    const json = (data) => route.fulfill({ json: { data } });
    if (path === '1/files') {
      if (request.method() !== 'POST') return json([]);
      const file = request.postDataJSON();
      state.attaches.push(file.file_name);
      return route.fulfill({
        status: 201,
        json: {
          data: {
            id: 40 + state.attaches.length,
            page_id: 1,
            name: file.file_name,
            mime_type: file.mime_type,
            size: file.size_bytes,
            created_at: '2026-10-01T10:00:00Z',
          },
        },
      });
    }
    if (request.method() !== 'GET') {
      const update = request.postDataJSON();
      state.writes.push({ path, body: update });
      if (path !== '1') return json(null);
      if (typeof update?.body === 'string') state.page.body = update.body;
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

const useScheme = async (page, scheme) => {
  await page.emulateMedia({ colorScheme: scheme });
  await page.addInitScript(
    (value) => localStorage.setItem('colorScheme', value),
    scheme,
  );
};

const savedBody = async (state, includes) => {
  await expect
    .poll(() => state.writes.at(-1)?.body?.body ?? '', { timeout: 10_000 })
    .toContain(includes);
  return state.writes.at(-1).body.body;
};

const expectNoWrites = async (page, state) => {
  await page.waitForTimeout(DEBOUNCE_MARGIN);
  expect(state.writes.map((write) => write.body?.body)).toEqual([]);
};

const openPage = async (page) => {
  await page.goto('/demo/pages/rich-page');
  await expect(
    page.locator('[data-testid="page-content"] .page-editor .ProseMirror'),
  ).toHaveCount(1);
  await expect(page.getByTestId('page-save-status')).toHaveText(
    /^Saved, version/,
  );
};

const figure = (page, index = 0) => page.locator('.pe-image').nth(index);
const picture = (page, index = 0) =>
  figure(page, index).locator('img.pe-image-img');
const tool = (page, action) =>
  page.locator(`.pe-image-toolbar [data-action="${action}"]`);

const loaded = async (page, index = 0) => {
  await expect
    .poll(() => picture(page, index).evaluate((img) => img.naturalWidth))
    .toBe(400);
};

const select = async (page, index = 0) => {
  await loaded(page, index);
  await picture(page, index).click();
  await expect(figure(page, index)).toHaveClass(/ProseMirror-selectednode/);
};

const handleCenter = async (page, side) => {
  const handle = figure(page).locator(`.pe-image-handle[data-side="${side}"]`);
  const box = await handle.boundingBox();
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
};

const drag = async (page, side, dx, dy, { finish = true } = {}) => {
  await picture(page).hover();
  const from = await handleCenter(page, side);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + dx / 2, from.y + dy / 2, { steps: 4 });
  await page.mouse.move(from.x + dx, from.y + dy, { steps: 4 });
  if (finish) await page.mouse.up();
};

const renderedWidth = (page) =>
  picture(page).evaluate((img) => img.getBoundingClientRect().width);

test.describe.configure({ timeout: 90_000 });

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
});

test('renders the node view contract with the size from the title', async ({
  page,
}) => {
  await setupPages(page, `First line\n\n${IMAGE}\n\nLast line\n`);
  await openPage(page);
  await loaded(page);
  await expect(figure(page)).toHaveAttribute('data-layout', 'center');
  await expect(figure(page).locator('.pe-image-handle')).toHaveCount(6);
  expect(await renderedWidth(page)).toBe(400);
  await expect(figure(page).locator('.pe-image-caption')).toBeHidden();
  await expect(figure(page).locator('.pe-image-toolbar')).toBeHidden();
  await select(page);
  await expect(figure(page).locator('.pe-image-toolbar')).toBeVisible();
  await expect(
    figure(page).locator('.pe-image-caption[contenteditable="true"]'),
  ).toHaveAttribute('data-placeholder', 'Write a caption');
  for (const action of [
    'align-left',
    'align-center',
    'align-right',
    'full-width',
    'download',
    'replace',
    'delete',
  ]) {
    await expect(tool(page, action)).toHaveAttribute('aria-label', /.+/);
  }
});

test('dragging the right side handle of a centered image resizes both sides', async ({
  page,
}) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n\nLast line\n`);
  await openPage(page);
  await loaded(page);
  await drag(page, 'right', 40, 0);
  expect(await savedBody(state, '=480x240')).toBe(
    'First line\n\n![](tmgr://file/1 " =480x240")\n\nLast line\n',
  );
  expect(await renderedWidth(page)).toBe(480);
  await expect(figure(page)).toHaveClass(/ProseMirror-selectednode/);
  expect(state.writes).toHaveLength(1);
});

test('a resize is one undo step', async ({ page }) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n`);
  await openPage(page);
  await loaded(page);
  await drag(page, 'right', 40, 0);
  await savedBody(state, '=480x240');
  await page.keyboard.press('ControlOrMeta+z');
  expect(await savedBody(state, '=400x200')).toBe(
    `First line\n\n${IMAGE}\n`,
  );
  await expect(page.locator('.pe-image')).toHaveCount(1);
  await expect.poll(() => renderedWidth(page)).toBe(400);
});

test('space is reserved from the title size before the file resolves', async ({
  page,
}) => {
  await setupPages(page, `First line\n\n${IMAGE}\n`);
  await page.route('**/api/files/1/content', () => {});
  await openPage(page);
  const box = await picture(page).boundingBox();
  expect(Math.round(box.width)).toBe(400);
  expect(Math.round(box.height)).toBe(200);
});

test('dragging the left side handle shrinks the image', async ({ page }) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n`);
  await openPage(page);
  await loaded(page);
  await drag(page, 'left', 50, 0);
  expect(await savedBody(state, '=300x150')).toBe(
    'First line\n\n![](tmgr://file/1 " =300x150")\n',
  );
});

test('a corner drag keeps the natural aspect ratio', async ({ page }) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n`);
  await openPage(page);
  await loaded(page);
  await drag(page, 'se', 10, 40);
  expect(await savedBody(state, '=480x240')).toBe(
    'First line\n\n![](tmgr://file/1 " =480x240")\n',
  );
});

test('Escape during a drag cancels the resize without a write', async ({
  page,
}) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n`);
  await openPage(page);
  await loaded(page);
  await drag(page, 'right', 60, 0, { finish: false });
  expect(await renderedWidth(page)).toBe(520);
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect.poll(() => renderedWidth(page)).toBe(400);
  await expect(page.locator('body.pe-resizing')).toHaveCount(0);
  await expectNoWrites(page, state);
});

test('double-clicking a handle resets the natural size', async ({ page }) => {
  const state = await setupPages(
    page,
    'First line\n\n![](tmgr://file/1 " =100x50")\n',
  );
  await openPage(page);
  await loaded(page);
  await picture(page).hover();
  const at = await handleCenter(page, 'right');
  await page.mouse.dblclick(at.x, at.y);
  expect(await savedBody(state, '=400x200')).toBe(
    'First line\n\n![](tmgr://file/1 " =400x200")\n',
  );
});

test('layout buttons write Outline layouts and full width hides handles', async ({
  page,
}) => {
  const state = await setupPages(
    page,
    'First line\n\n![](tmgr://file/1 "Shot =400x200")\n',
  );
  await openPage(page);
  await select(page);

  await tool(page, 'align-right').click();
  expect(await savedBody(state, 'right-50')).toBe(
    'First line\n\n![](tmgr://file/1 "right-50 Shot =400x200")\n',
  );
  await expect(figure(page)).toHaveAttribute('data-layout', 'right-50');
  await expect(tool(page, 'align-right')).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  expect(
    await figure(page).evaluate((el) => getComputedStyle(el).float),
  ).toBe('right');

  await tool(page, 'full-width').click();
  expect(await savedBody(state, 'full-width')).toBe(
    'First line\n\n![](tmgr://file/1 "full-width Shot =400x200")\n',
  );
  await expect(figure(page)).toHaveAttribute('data-layout', 'full-width');
  await expect(figure(page).locator('.pe-image-handle:visible')).toHaveCount(0);
  if (shotDir)
    await page
      .locator('[data-testid="page-content"]')
      .screenshot({ path: `${shotDir}/image-full.png` });
  const column = await figure(page).evaluate(
    (el) => el.parentElement.clientWidth,
  );
  expect(await renderedWidth(page)).toBe(column);

  await tool(page, 'full-width').click();
  expect(await savedBody(state, '"Shot =400x200"')).toBe(
    'First line\n\n![](tmgr://file/1 "Shot =400x200")\n',
  );
  await expect(figure(page)).toHaveAttribute('data-layout', 'center');

  await tool(page, 'align-left').click();
  expect(await savedBody(state, 'left-50')).toBe(
    'First line\n\n![](tmgr://file/1 "left-50 Shot =400x200")\n',
  );
  await tool(page, 'align-center').click();
  expect(await savedBody(state, '"Shot =400x200"')).toBe(
    'First line\n\n![](tmgr://file/1 "Shot =400x200")\n',
  );
  await expect(figure(page).locator('.pe-image-handle')).toHaveCount(6);
  await expect(figure(page).locator('.pe-image-handle:visible')).toHaveCount(6);
});

test('a floated image resizes by the plain delta', async ({ page }) => {
  const state = await setupPages(
    page,
    'First line\n\n![](tmgr://file/1 "left-50 =200x100")\n\nText beside\n',
  );
  await openPage(page);
  await loaded(page);
  if (shotDir)
    await page
      .locator('[data-testid="page-content"]')
      .screenshot({ path: `${shotDir}/image-float.png` });
  await drag(page, 'right', 30, 0);
  expect(await savedBody(state, '=230x115')).toBe(
    'First line\n\n![](tmgr://file/1 "left-50 =230x115")\n\nText beside\n',
  );
});

test('a caption being typed is saved without leaving the caption', async ({
  page,
}) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n`);
  await openPage(page);
  await select(page);
  const caption = figure(page).locator('.pe-image-caption');
  await caption.click();
  await page.keyboard.type('Still typing');
  expect(await savedBody(state, 'Still typing')).toBe(
    'First line\n\n![Still typing](tmgr://file/1 " =400x200")\n',
  );
  await expect(caption).toBeFocused();
  await expect(caption).toHaveText('Still typing');
});

test('editing the caption writes the alt and keeps typing out of the doc', async ({
  page,
}) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n`);
  await openPage(page);
  await select(page);
  const caption = figure(page).locator('.pe-image-caption');
  await expect(caption).toBeVisible();
  await caption.click();
  await page.keyboard.type('My caption');
  await page.keyboard.press('Enter');
  expect(await savedBody(state, 'My caption')).toBe(
    'First line\n\n![My caption](tmgr://file/1 " =400x200")\n',
  );
  await page.keyboard.type('After');
  expect(await savedBody(state, 'After')).toBe(
    'First line\n\n![My caption](tmgr://file/1 " =400x200")\n\nAfter\n',
  );
  await expect(caption).toHaveText('My caption');
  if (shotDir)
    await page
      .locator('[data-testid="page-content"]')
      .screenshot({ path: `${shotDir}/image-caption.png` });
  await page.locator('.ProseMirror p', { hasText: 'First line' }).click();
  await expect(caption).toBeVisible();
});

test('dragging across caption text selects it instead of moving the image', async ({
  page,
}) => {
  const state = await setupPages(
    page,
    `First line\n\n![Some caption words](tmgr://file/1 " =400x200")\n`,
  );
  await openPage(page);
  await select(page);
  const box = await figure(page).locator('.pe-image-caption').boundingBox();
  await page.mouse.move(box.x + box.width / 2 - 50, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 50, box.y + box.height / 2, {
    steps: 6,
  });
  await page.mouse.up();
  expect(
    (await page.evaluate(() => window.getSelection().toString())).length,
  ).toBeGreaterThan(3);
  await expect(figure(page).locator('.pe-image-caption')).toBeFocused();
  await expect(page.locator('.pe-image')).toHaveCount(1);
  await expectNoWrites(page, state);
});

test('Backspace in an empty caption selects the image', async ({ page }) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n`);
  await openPage(page);
  await select(page);
  await figure(page).locator('.pe-image-caption').click();
  await page.keyboard.press('Backspace');
  await expect(figure(page)).toHaveClass(/ProseMirror-selectednode/);
  await expect
    .poll(() => page.evaluate(() => document.activeElement?.className))
    .toContain('ProseMirror');
  await expectNoWrites(page, state);
});

test('a legacy filename alt is not a caption and the page saves nothing', async ({
  page,
}) => {
  const state = await setupPages(
    page,
    'First line\n\n![shot.png](tmgr://file/1)\n\nLast line\n',
  );
  await openPage(page);
  await loaded(page);
  await expect(picture(page)).toHaveAttribute('alt', 'shot.png');
  await select(page);
  const caption = figure(page).locator('.pe-image-caption');
  await expect(caption).toHaveText('');
  await caption.click();
  await page.locator('.ProseMirror p', { hasText: 'Last line' }).click();
  await expect(caption).toBeHidden();
  await expectNoWrites(page, state);
  await page.keyboard.type(' edited');
  expect(await savedBody(state, 'edited')).toBe(
    'First line\n\n![shot.png](tmgr://file/1)\n\nLast line edited\n',
  );
});

test('Replace picks one file and swaps the image source', async ({ page }) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n`);
  await openPage(page);
  await select(page);
  const [chooser] = await Promise.all([
    page.waitForEvent('filechooser'),
    tool(page, 'replace').click(),
  ]);
  expect(chooser.isMultiple()).toBe(false);
  await chooser.setFiles({
    name: 'new.png',
    mimeType: 'image/png',
    buffer: PNG,
  });
  await expect.poll(() => state.attaches).toEqual(['new.png']);
  expect(await savedBody(state, 'tmgr://file/41')).toBe(
    'First line\n\n![](tmgr://file/41 " =400x200")\n',
  );
});

test('Delete removes the image', async ({ page }) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n\nLast\n`);
  await openPage(page);
  await select(page);
  await tool(page, 'delete').click();
  await expect(page.locator('.pe-image')).toHaveCount(0);
  await expect
    .poll(() => state.writes.at(-1)?.body?.body ?? '')
    .not.toContain('tmgr://file');
});

test('dragging the image moves the node', async ({ page }) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n\nLast\n`);
  await openPage(page);
  await loaded(page);
  const target = page.locator('.ProseMirror p', { hasText: 'First line' });
  const box = await target.boundingBox();
  await picture(page).dragTo(target, {
    targetPosition: { x: 2, y: box.height / 2 },
  });
  await expect
    .poll(() => state.writes.at(-1)?.body?.body ?? '', { timeout: 10_000 })
    .toBe(`${IMAGE}\n\nFirst line\n\nLast\n`);
  await expect(page.locator('.pe-image')).toHaveCount(1);
});

test('Enter on a selected image adds a paragraph below and keeps the image', async ({
  page,
}) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n\nLast\n`);
  await openPage(page);
  await select(page);
  await page.keyboard.press('Enter');
  await page.keyboard.type('Below');
  expect(await savedBody(state, 'Below')).toBe(
    `First line\n\n${IMAGE}\n\nBelow\n\nLast\n`,
  );
});

test('ArrowRight after an image at the end of a paragraph types after it', async ({
  page,
}) => {
  const state = await setupPages(page, `First line\n\n${IMAGE}\n\nLast\n`);
  await openPage(page);
  await select(page);
  const gap = await page.evaluate(() => {
    const caption = document.querySelector('.pe-image-caption');
    const last = [...document.querySelectorAll('.ProseMirror p')].at(-1);
    return (
      last.getBoundingClientRect().top - caption.getBoundingClientRect().bottom
    );
  });
  expect(gap).toBeLessThan(20);
  await page.keyboard.press('ArrowRight');
  if (shotDir)
    await page
      .locator('[data-testid="page-content"]')
      .screenshot({ path: `${shotDir}/image-caret.png` });
  await page.keyboard.type('tail');
  if (shotDir)
    await page
      .locator('[data-testid="page-content"]')
      .screenshot({ path: `${shotDir}/image-tail.png` });
  expect(await savedBody(state, 'tail')).toBe(
    `First line\n\n${IMAGE}tail\n\nLast\n`,
  );
});

test('the lightbox opens on a selected image and pages through images', async ({
  page,
}) => {
  const state = await setupPages(
    page,
    `First line\n\n${IMAGE}\n\n![Second](tmgr://file/2)\n`,
  );
  await openPage(page);
  await loaded(page, 1);
  await select(page);
  await picture(page).click();
  const lightbox = page.locator('.pe-lightbox');
  await expect(lightbox).toBeVisible();
  await expect(lightbox).toHaveAttribute('role', 'dialog');
  await expect(lightbox).toHaveAttribute('aria-modal', 'true');
  await expect(lightbox.locator('[data-action="close"]')).toBeFocused();
  const sources = await page
    .locator('img.pe-image-img')
    .evaluateAll((imgs) => imgs.map((img) => img.src));
  const shown = lightbox.locator('img');
  await expect(shown).toHaveAttribute('src', sources[0]);
  await expect(lightbox.locator('[data-action="prev"]')).toBeHidden();
  await page.keyboard.press('ArrowRight');
  await expect(shown).toHaveAttribute('src', sources[1]);
  await expect(lightbox.locator('[data-action="next"]')).toBeHidden();
  await page.keyboard.press('ArrowLeft');
  await expect(shown).toHaveAttribute('src', sources[0]);
  await lightbox.locator('[data-action="next"]').click();
  await expect(shown).toHaveAttribute('src', sources[1]);
  await expect(lightbox.locator('[data-action="download"]')).toBeVisible();
  if (shotDir) await page.screenshot({ path: `${shotDir}/lightbox.png` });
  expect(
    await page.evaluate(() => getComputedStyle(document.documentElement).overflow),
  ).toBe('hidden');
  await page.keyboard.press('Escape');
  await expect(lightbox).toHaveCount(0);
  await expect(page.locator('.pe-image')).toHaveCount(2);
  await expect
    .poll(() => page.evaluate(() => document.activeElement?.className))
    .toContain('ProseMirror');

  await page
    .locator('.ProseMirror p', { hasText: 'First line' })
    .click({ position: { x: 4, y: 4 } });
  await select(page);
  await page.keyboard.press(' ');
  await expect(lightbox).toBeVisible();
  await lightbox.locator('[data-action="close"]').click();
  await expect(lightbox).toHaveCount(0);
  await expect(page.locator('.pe-image')).toHaveCount(2);
  await expectNoWrites(page, state);
});

test('download fetches the stored file', async ({ page }) => {
  await setupPages(page, `First line\n\n![shot.png](tmgr://file/1)\n`);
  await openPage(page);
  await select(page);
  const [download] = await Promise.all([
    page.waitForEvent('download'),
    tool(page, 'download').click(),
  ]);
  expect(download.suggestedFilename()).toBe('shot.png');
});

test('a missing file shows the failed state without handles', async ({
  page,
}) => {
  await setupPages(page, `First line\n\n![](tmgr://file/9 " =400x200")\n`, {
    missing: [9],
  });
  await openPage(page);
  await expect(figure(page)).toHaveClass(/pe-image-failed/);
  const failure = figure(page).locator('.pe-image-error');
  await expect(failure).toBeVisible();
  await expect(failure).toContainText('Image failed to load');
  expect((await failure.boundingBox()).height).toBe(80);
  await expect(picture(page)).toBeHidden();
  if (shotDir)
    await page
      .locator('[data-testid="page-content"]')
      .screenshot({ path: `${shotDir}/image-failed.png` });
  await expect(figure(page).locator('.pe-image-handle:visible')).toHaveCount(0);
});

for (const scheme of ['light', 'dark']) {
  test(`image controls are legible in ${scheme}`, async ({ page }) => {
    await useScheme(page, scheme);
    await setupPages(page, `First line\n\n${IMAGE}\n`);
    await openPage(page);
    await select(page);
    await picture(page).hover();
    const colors = await figure(page).evaluate((el) => {
      const toolbar = el.querySelector('.pe-image-toolbar');
      const pill = getComputedStyle(
        el.querySelector('.pe-image-handle[data-side="right"]'),
        '::after',
      );
      return {
        dark: document.documentElement.classList.contains('dark'),
        bar: getComputedStyle(toolbar).backgroundColor,
        icon: getComputedStyle(toolbar.querySelector('button')).color,
        pill: pill.backgroundColor,
        outline: getComputedStyle(el.querySelector('img')).outlineColor,
        page: getComputedStyle(document.body).backgroundColor,
      };
    });
    expect(colors.dark).toBe(scheme === 'dark');
    expect(colors.icon).not.toBe(colors.bar);
    expect(colors.bar).not.toBe('rgba(0, 0, 0, 0)');
    expect(colors.pill).not.toBe('rgba(0, 0, 0, 0)');
    expect(colors.outline).not.toBe(colors.page);
    if (shotDir) {
      await page
        .locator('[data-testid="page-content"]')
        .screenshot({ path: `${shotDir}/image-view-${scheme}.png` });
    }
  });
}
