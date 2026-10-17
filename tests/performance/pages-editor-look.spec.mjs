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

const SECTION = [
  '<!-- tmgr:section id="x" owner="system" -->',
  '## Managed summary',
  '- Generated line one',
  '- Generated line for [Original task](tmgr://task/1)',
  '<!-- /tmgr:section -->',
].join('\n');

const RICH_BODY = [
  '# Project handbook',
  '',
  'Intro paragraph with **bold**, *italic*, ~~strike~~ and `inline code`, plus a [normal link](https://example.com/docs).',
  '',
  'Linked task [Original task](tmgr://task/1) and page [Empty page](tmgr://page/2).',
  '',
  '## Lists',
  '',
  '- First bullet',
  '- Second bullet',
  '',
  '1. First step',
  '2. Second step',
  '',
  '- [x] Done item',
  '- [ ] Open item',
  '',
  SECTION,
  '',
  '### Code and data',
  '',
  '```ts',
  'const answer: number = 42;',
  'console.log(answer);',
  '```',
  '',
  '| Name | Value |',
  '| --- | --- |',
  '| Alpha | 1 |',
  '| Beta | 2 |',
  '',
  '---',
  '',
  '> A quoted line of text.',
  '',
  '![diagram](tmgr://file/7)',
  '',
  'Closing paragraph.',
  '',
].join('\n');

const author = { kind: 'user', id: 1, name: 'Test User' };

const setupPages = async (page, { richBody = RICH_BODY } = {}) => {
  await mockApp(page);
  const state = {
    pages: [
      {
        id: 1,
        title: 'Rich page',
        slug: 'rich-page',
        body: richBody,
        version: 3,
      },
      { id: 2, title: 'Empty page', slug: 'empty-page', body: '', version: 1 },
    ],
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
    const method = request.method();
    const path = new URL(request.url()).pathname
      .replace('/api/pages', '')
      .replace(/^\//, '');
    const json = (data) => route.fulfill({ json: { data } });
    if (method !== 'GET') {
      const body = request.postDataJSON();
      state.writes.push({ method, path, body });
      const target = state.pages.find(
        (p) => path.split('/')[0] === String(p.id),
      );
      if (!target) return json(null);
      if (typeof body?.body === 'string') target.body = body.body;
      target.version += 1;
      return json(full(target));
    }
    if (path === 'tree' || path === '') return json(state.pages.map(summary));
    const target = state.pages.find(
      (p) => p.slug === path || String(p.id) === path,
    );
    if (target) return json(full(target));
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

const editors = (page) =>
  page.locator('[data-testid="page-content"] .page-editor .ProseMirror');

const openPage = async (page, slug, editorCount) => {
  await page.goto(`/demo/pages/${slug}`);
  await expect(editors(page)).toHaveCount(editorCount);
  await expect(page.getByTestId('page-save-status')).toHaveText(
    /^Saved, version/,
  );
};

const DEBOUNCE_MARGIN = 2500;

test.describe.configure({ timeout: 90_000 });

const caretAtEnd = async (paragraph) => {
  await paragraph.click();
  await paragraph.evaluate((el) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
  });
  await paragraph.page().waitForTimeout(100);
};

test.describe('Pages editor round-trip', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
  });

  test('opening a page with every construct saves nothing and stays clean', async ({
    page,
  }) => {
    const state = await setupPages(page);
    await openPage(page, 'rich-page', 2);
    await expect(page.getByTestId('managed-section')).toBeVisible();
    await expect(
      page.locator('.page-editor img[alt="diagram"]'),
    ).toHaveAttribute('src', /^blob:/);
    await page.waitForTimeout(DEBOUNCE_MARGIN);
    expect(state.writes).toEqual([]);
    await expect(page.getByTestId('page-save-status')).toHaveText(
      'Saved, version 3',
    );
  });

  // Each construct sits alone above a trailing paragraph. Typing one character there makes the
  // editor emit its markdown, so the saved body shows whether the untouched construct survived.
  const CONSTRUCTS = {
    h1: '# Heading one',
    h2: '## Heading two',
    h3: '### Heading three',
    'inline marks':
      'Text with **bold**, *italic*, ~~strike~~ and `inline code`.',
    'https link': 'A [normal link](https://example.com/docs) here.',
    'tmgr links':
      'See [Original task](tmgr://task/1) and [Empty page](tmgr://page/2).',
    'bullet list': '- First bullet\n- Second bullet',
    'ordered list': '1. First step\n2. Second step',
    'task list': '- [x] Done item\n- [ ] Open item',
    'fenced code': '```ts\nconst answer: number = 42;\n```',
    table: '| Name | Value |\n| --- | --- |\n| Alpha | 1 |',
    hr: 'Above the rule\n\n---',
    blockquote: '> A quoted line of text.',
    image: '![diagram](tmgr://file/7)',
    'managed section': `Intro line.\n\n${SECTION}`,
  };

  for (const [name, snippet] of Object.entries(CONSTRUCTS)) {
    test(`edit next to ${name} keeps it byte-identical`, async ({ page }) => {
      const body = `${snippet}\n\nEdit here\n`;
      const state = await setupPages(page, { richBody: body });
      await openPage(page, 'rich-page', name === 'managed section' ? 2 : 1);
      await page.waitForTimeout(DEBOUNCE_MARGIN);
      expect(state.writes, 'no save on open').toEqual([]);
      await caretAtEnd(
        page.locator('.page-editor .ProseMirror p', { hasText: 'Edit here' }),
      );
      await page.keyboard.type('.');
      await expect
        .poll(() => state.writes.length, { timeout: 10_000 })
        .toBeGreaterThan(0);
      expect(state.writes[0].body.body).toBe(`${snippet}\n\nEdit here.\n`);
    });
  }
});

const VIEWPORTS = {
  desktop: { width: 1440, height: 900 },
  phone: { width: 390, height: 844 },
};

const overlaps = (a, b) =>
  a.x < b.x + b.width &&
  b.x < a.x + a.width &&
  a.y < b.y + b.height &&
  b.y < a.y + a.height;

const savedBody = async (state) => {
  await expect
    .poll(() => state.writes.length, { timeout: 10_000 })
    .toBeGreaterThan(0);
  return state.writes.at(-1).body.body;
};

for (const scheme of ['light', 'dark']) {
  for (const [device, viewport] of Object.entries(VIEWPORTS)) {
    test(`editor look ${scheme} ${device}`, async ({ page }) => {
      const shot = async (scene, options = {}) => {
        if (!shotDir) return;
        await page.waitForTimeout(300);
        await page.screenshot({
          path: `${shotDir}/${scene}-${scheme}-${device}.png`,
          ...options,
        });
      };
      await page.setViewportSize(viewport);
      await useScheme(page, scheme);
      await setupPages(page);
      await openPage(page, 'rich-page', 2);
      expect(
        await page.evaluate(() =>
          document.documentElement.classList.contains('dark'),
        ),
      ).toBe(scheme === 'dark');
      await expect(
        page.locator('.page-editor img[alt="diagram"]'),
      ).toHaveAttribute('src', /^blob:/);
      await expect(
        page.locator('.page-editor .tok-keyword').first(),
      ).toBeVisible();
      await shot('rich', { fullPage: true });

      const intro = page.locator('.page-editor .ProseMirror p', {
        hasText: 'Intro paragraph',
      });
      await intro.scrollIntoViewIfNeeded();
      await intro.hover();
      const handle = page.locator(
        '.page-editor .milkdown-block-handle[data-show="true"]',
      );
      if (device === 'desktop') {
        await expect(handle).toBeVisible();
        const handleBox = await handle.boundingBox();
        const textBox = await intro.boundingBox();
        expect(
          handleBox.x + handleBox.width,
          'handle sits left of the text',
        ).toBeLessThanOrEqual(textBox.x);
      } else {
        await expect(handle).toBeHidden();
      }
      await shot('block-handle');

      const code = page.locator('.page-editor .milkdown-code-block').first();
      await code.scrollIntoViewIfNeeded();
      await expect(code).toContainText('const answer');
      await code.hover();
      await expect(code.locator('.language-button')).toHaveText(/ts/i);
      await shot('code');
      await page.mouse.move(0, 0);

      await intro.scrollIntoViewIfNeeded();
      await intro.locator('strong').dblclick();
      const toolbar = page.locator(
        '.page-editor .milkdown-toolbar[data-show="true"]',
      );
      await expect(toolbar).toBeVisible();
      await expect(
        toolbar.locator('[data-toolbar-item="make-task"]'),
      ).toBeVisible();
      await expect(page.getByTestId('selection-to-task')).toHaveCount(0);
      await shot('selection');

      await intro.locator('a').first().hover();
      await expect(
        page.locator('.page-editor .milkdown-link-preview[data-show="true"]'),
      ).toBeVisible();
      await shot('link-preview');

      const closing = page.locator('.page-editor .ProseMirror p', {
        hasText: 'Closing paragraph',
      });
      await closing.scrollIntoViewIfNeeded();
      await caretAtEnd(closing);
      await page.keyboard.press('Enter');
      await page.keyboard.type('/');
      const menu = page.locator(
        '.page-editor .milkdown-slash-menu[data-show="true"]',
      );
      await expect(menu).toBeVisible();
      const menuBox = await menu.boundingBox();
      expect(menuBox.y).toBeGreaterThanOrEqual(0);
      expect(menuBox.y + menuBox.height).toBeLessThanOrEqual(viewport.height);
      expect(menuBox.x + menuBox.width).toBeLessThanOrEqual(viewport.width);
      await shot('slash');

      await page.keyboard.press('Escape');
      await code.scrollIntoViewIfNeeded();
      await code.hover();
      await code.locator('.language-button').click();
      await expect(code.locator('.language-list')).toBeVisible();
      await shot('code-languages');
      await page.goto('/demo/pages/empty-page');
      await expect(editors(page)).toHaveCount(1);
      await editors(page).click();
      await expect(editors(page)).toBeFocused();
      await shot('empty');
    });
  }
}

test('Make a task sits in the selection toolbar and opens the dialog', async ({
  page,
}) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  await setupPages(page);
  await openPage(page, 'rich-page', 2);
  const intro = page.locator('.page-editor .ProseMirror p', {
    hasText: 'Intro paragraph',
  });
  await intro.locator('strong').dblclick();
  const toolbar = page.locator(
    '.page-editor .milkdown-toolbar[data-show="true"]',
  );
  await expect(toolbar).toBeVisible();
  const toolbarBox = await toolbar.boundingBox();
  const wordBox = await intro.locator('strong').boundingBox();
  expect(
    overlaps(toolbarBox, wordBox),
    'toolbar does not cover the selection',
  ).toBe(false);
  await expect(page.getByTestId('selection-to-task')).toHaveCount(0);
  await toolbar.locator('[data-toolbar-item="make-task"]').click();
  const dialog = page.getByRole('dialog', { name: 'Make a task' });
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('blockquote')).toHaveText('bold');
});

test('Make a task button for a managed section sits above the selection', async ({
  page,
}) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  const body = [
    'Intro line.',
    '',
    '<!-- tmgr:section id="a" owner="agents" -->',
    'Agents wrote this first line of notes for the team to read.',
    '',
    'Second line right below it.',
    '<!-- /tmgr:section -->',
    '',
  ].join('\n');
  await setupPages(page, { richBody: body });
  await openPage(page, 'rich-page', 2);
  const first = page.getByTestId('managed-section').locator('p').first();
  const next = page.getByTestId('managed-section').locator('p').nth(1);
  await first.evaluate((el) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
  });
  await first.dispatchEvent('mouseup');
  const button = page.getByTestId('selection-to-task');
  await expect(button).toBeVisible();
  const b = await button.boundingBox();
  expect(
    overlaps(b, await first.boundingBox()),
    'covers the selected line',
  ).toBe(false);
  expect(overlaps(b, await next.boundingBox()), 'covers the next line').toBe(
    false,
  );
});

test('rich page does not overflow horizontally at phone width', async ({
  page,
}) => {
  await page.setViewportSize(VIEWPORTS.phone);
  await setupPages(page);
  await openPage(page, 'rich-page', 2);
  const overflow = () =>
    page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth,
    );
  expect(await overflow(), 'on load').toBeLessThanOrEqual(0);
  const intro = page.locator('.page-editor .ProseMirror p', {
    hasText: 'Intro paragraph',
  });
  await intro.hover();
  await expect(
    page.locator('.page-editor .milkdown-block-handle[data-show="true"]'),
  ).toBeHidden();
  expect(await overflow(), 'after hovering a block').toBeLessThanOrEqual(0);
  const text = await intro.boundingBox();
  expect(text.width, 'text column uses the phone width').toBeGreaterThan(300);
});

test('theme toggle recolors the editor without a remount', async ({ page }) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  await useScheme(page, 'light');
  await setupPages(page);
  await openPage(page, 'rich-page', 2);
  await expect(page.locator('.page-editor .tok-keyword').first()).toBeVisible();
  const read = () =>
    page.evaluate(() => {
      const editor = document.querySelector('.page-editor .milkdown');
      const code = document.querySelector('.page-editor .milkdown-code-block');
      const keyword = document.querySelector('.page-editor .tok-keyword');
      return {
        text: getComputedStyle(editor).color,
        code: getComputedStyle(code).backgroundColor,
        keyword: getComputedStyle(keyword).color,
      };
    });
  await page.evaluate(() => {
    window.__editorNode = document.querySelector('.page-editor .milkdown');
  });
  const light = await read();
  await page.evaluate(() => document.documentElement.classList.add('dark'));
  const dark = await read();
  expect(dark.text).not.toBe(light.text);
  expect(dark.code).not.toBe(light.code);
  expect(dark.keyword).not.toBe(light.keyword);
  expect(
    await page.evaluate(
      () =>
        window.__editorNode ===
        document.querySelector('.page-editor .milkdown'),
    ),
  ).toBe(true);
  expect(await page.locator('style[data-pages-editor-theme]').count()).toBe(0);
});

test('slash menu turns the line into a heading with the keyboard', async ({
  page,
}) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  const state = await setupPages(page, { richBody: 'First line\n' });
  await openPage(page, 'rich-page', 1);
  await caretAtEnd(
    page.locator('.page-editor .ProseMirror p', { hasText: 'First line' }),
  );
  await page.keyboard.press('Enter');
  await page.keyboard.type('/');
  const menu = page.locator(
    '.page-editor .milkdown-slash-menu[data-show="true"]',
  );
  await expect(menu).toBeVisible();
  await expect(menu.locator('li.hover')).toHaveText('Text');
  await page.keyboard.press('ArrowDown');
  await expect(menu.locator('li.hover')).toHaveText('Heading 1');
  await page.keyboard.press('Enter');
  await expect(menu).toBeHidden();
  await page.keyboard.type('New heading');
  await expect
    .poll(async () => (await savedBody(state)).includes('# New heading'))
    .toBe(true);
  expect(await savedBody(state)).toBe('First line\n\n# New heading\n');
});

test('checking a task item saves - [x]', async ({ page }) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  const state = await setupPages(page, {
    richBody: '- [x] Done item\n- [ ] Open item\n',
  });
  await openPage(page, 'rich-page', 1);
  const open = page.locator('.page-editor .milkdown-list-item-block', {
    hasText: 'Open item',
  });
  await open.locator('.label-wrapper').click();
  await expect
    .poll(() => state.writes.length, { timeout: 10_000 })
    .toBeGreaterThan(0);
  expect(await savedBody(state)).toBe('- [x] Done item\n- [x] Open item\n');
});

test('[[ opens the page menu, Enter inserts a page link', async ({ page }) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  const state = await setupPages(page, { richBody: 'See\n' });
  await openPage(page, 'rich-page', 1);
  await caretAtEnd(
    page.locator('.page-editor .ProseMirror p', { hasText: 'See' }),
  );
  await page.keyboard.type(' [[zzz');
  const menu = page.getByTestId('mention-menu');
  await expect(menu).toBeHidden();
  for (let i = 0; i < 5; i++) await page.keyboard.press('Backspace');
  await page.keyboard.type('[[Emp');
  await expect(menu).toBeVisible();
  await expect(
    menu.getByRole('option', { name: /Empty page/ }),
  ).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Enter');
  await expect(menu).toBeHidden();
  await expect(page.locator('.page-editor a.tmgr-chip-page')).toHaveText(
    'Empty page',
  );
  await expect
    .poll(async () =>
      (await savedBody(state)).startsWith('See [Empty page](tmgr://page/2)'),
    )
    .toBe(true);
});

test('empty page shows one placeholder with the / hint', async ({ page }) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  await setupPages(page);
  await page.goto('/demo/pages/empty-page');
  await expect(editors(page)).toHaveCount(1);
  const placeholders = () =>
    page.evaluate(() =>
      [...document.querySelectorAll('.page-editor .crepe-placeholder')]
        .map((el) => getComputedStyle(el, '::before').content)
        .filter((content) => content && content !== 'none'),
    );
  expect(await placeholders()).toEqual([
    '"Type / for blocks, @ to link, [[ for a page"',
  ]);
  const area = await editors(page).boundingBox();
  expect(
    area.height,
    'blank area below the first line is part of the editor',
  ).toBeGreaterThan(100);
  await page.mouse.click(area.x + 40, area.y + area.height - 10);
  await expect(editors(page)).toBeFocused();
  expect((await placeholders()).length).toBe(1);
});

test('a page with sections shows the placeholder only on the focused empty line', async ({
  page,
}) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  await setupPages(page, {
    richBody: `${SECTION}\n${SECTION.replace('id="x"', 'id="y"')}\n`,
  });
  await openPage(page, 'rich-page', 3);
  const visible = () =>
    page.evaluate(
      () =>
        [
          ...document.querySelectorAll('.page-editor .crepe-placeholder'),
        ].filter((el) => {
          const content = getComputedStyle(el, '::before').content;
          return content && content !== 'none';
        }).length,
    );
  expect(await visible()).toBe(0);
  await editors(page).last().click();
  expect(await visible()).toBe(1);
});

test('managed section does not jump while the editors above it mount', async ({
  page,
}) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  await page.addInitScript(() => {
    window.__sectionTops = [];
    const sample = () => {
      const el = document.querySelector('[data-testid="managed-section"]');
      if (el && getComputedStyle(el).visibility === 'visible') {
        const top = Math.round(el.getBoundingClientRect().top);
        if (window.__sectionTops.at(-1) !== top) window.__sectionTops.push(top);
      }
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  });
  await setupPages(page);
  await page.route('**/api/tasks/1**', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    return route.fallback();
  });
  const positions = async () => {
    await openPage(page, 'rich-page', 2);
    await page.waitForTimeout(500);
    return page.evaluate(() => window.__sectionTops);
  };
  const tops = await positions();
  test.info().annotations.push({
    type: 'visible section tops',
    description: tops.join(','),
  });
  expect(tops).toHaveLength(1);

  await page.addInitScript(() => {
    const style = document.createElement('style');
    style.textContent = '.page-doc { visibility: visible !important; }';
    document.addEventListener('DOMContentLoaded', () =>
      document.head.appendChild(style),
    );
  });
  const withoutFix = await positions();
  test.info().annotations.push({
    type: 'visible section tops without the fix',
    description: withoutFix.join(','),
  });
  expect(
    withoutFix.length,
    'the scenario does jump when the column is shown early',
  ).toBeGreaterThan(1);
});

test.describe('QA verification', () => {
  test('editing before and after the managed section keeps the rest of the page byte-identical', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    const state = await setupPages(page);
    await openPage(page, 'rich-page', 2);
    await caretAtEnd(
      page.locator('.page-editor .ProseMirror p', { hasText: 'Linked task' }),
    );
    await page.keyboard.type('!');
    await expect
      .poll(async () => (await savedBody(state)).includes('(tmgr://page/2).!'))
      .toBe(true);
    const first = RICH_BODY.replace('(tmgr://page/2).', '(tmgr://page/2).!');
    expect(await savedBody(state)).toBe(first);
    await caretAtEnd(
      page.locator('.page-editor .ProseMirror p', {
        hasText: 'Closing paragraph',
      }),
    );
    await page.keyboard.type('!');
    await expect
      .poll(async () =>
        (await savedBody(state)).includes('Closing paragraph.!'),
      )
      .toBe(true);
    expect(await savedBody(state)).toBe(
      first.replace('Closing paragraph.', 'Closing paragraph.!'),
    );
  });

  const insertMention = async (page) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    const state = await setupPages(page, { richBody: 'Ask\n' });
    await page.route('**/api/tasks/search**', async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 800));
      return route.fulfill({ json: [{ id: 1, title: 'Original task' }] });
    });
    await openPage(page, 'rich-page', 1);
    await caretAtEnd(
      page.locator('.page-editor .ProseMirror p', { hasText: 'Ask' }),
    );
    await page.keyboard.type(' @Ori');
    const menu = page.getByTestId('mention-menu');
    await expect(menu).toContainText('Searching...');
    const option = menu.getByRole('option', { name: /Original task/ });
    await expect(option).toBeVisible();
    await option.dispatchEvent('mousedown');
    await expect(menu).toBeHidden();
    const chip = page.locator('.page-editor a.tmgr-chip-task');
    await expect(chip).toHaveText('Original task');
    await expect
      .poll(async () =>
        (
          await savedBody(state)
        ).startsWith('Ask [Original task](tmgr://task/1)'),
      )
      .toBe(true);
    return chip;
  };

  test('@ opens the mention menu with a loading state, a picked task is saved as a tmgr://task link', async ({
    page,
  }) => {
    await insertMention(page);
  });

  test('a freshly picked mention chip carries its hover title', async ({
    page,
  }) => {
    const chip = await insertMention(page);
    await expect(chip).toHaveAttribute('data-current-title', 'Original task', {
      timeout: 3000,
    });
  });

  test('code language picker stays inside the phone viewport', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.phone);
    await setupPages(page);
    await openPage(page, 'rich-page', 2);
    const code = page.locator('.page-editor .milkdown-code-block').first();
    await code.scrollIntoViewIfNeeded();
    await code.hover();
    await code.locator('.language-button').click();
    const list = code.locator('.language-list');
    await expect(list).toBeVisible();
    const box = await list.boundingBox();
    expect(box.x + box.width, 'language list right edge').toBeLessThanOrEqual(
      VIEWPORTS.phone.width,
    );
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth - window.innerWidth,
      ),
      'no horizontal overflow',
    ).toBeLessThanOrEqual(0);
  });

  test('code language picker opens right-aligned under its button on desktop', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await setupPages(page);
    await openPage(page, 'rich-page', 2);
    const code = page.locator('.page-editor .milkdown-code-block').first();
    await code.scrollIntoViewIfNeeded();
    await code.hover();
    const button = code.locator('.language-button');
    await button.click();
    const list = code.locator('.language-list');
    await expect(list).toBeVisible();
    const listBox = await list.boundingBox();
    const buttonBox = await button.boundingBox();
    const codeBox = await code.boundingBox();
    expect(listBox.y, 'below the button').toBeGreaterThanOrEqual(
      buttonBox.y + buttonBox.height - 1,
    );
    expect(
      listBox.x + listBox.width,
      'inside the code block',
    ).toBeLessThanOrEqual(codeBox.x + codeBox.width);
    expect(listBox.x, 'inside the code block').toBeGreaterThanOrEqual(
      codeBox.x,
    );
    expect(
      await list.evaluate((el) => {
        el.scrollIntoView({ block: 'center' });
        const r = el.getBoundingClientRect();
        const hit = document.elementFromPoint(r.left + 10, r.top + 10);
        return el.contains(hit)
          ? 'list'
          : `${hit?.tagName}.${hit?.className} @${Math.round(
              r.left,
            )},${Math.round(r.top)} ${Math.round(r.width)}x${Math.round(
              r.height,
            )}`;
      }),
      'not clipped or covered',
    ).toBe('list');
    await list.getByText('Python', { exact: true }).click();
    await expect(button).toHaveText(/python/i);
  });

  test('block handle stays visible while hovered, + opens the menu, hides when the pointer leaves', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    const state = await setupPages(page, {
      richBody: 'Alpha line\n\nBeta line\n\nGamma line\n',
    });
    await openPage(page, 'rich-page', 1);
    const para = (text) =>
      page.locator('.page-editor .ProseMirror p', { hasText: text });
    const handle = page.locator(
      '.page-editor .milkdown-block-handle[data-show="true"]',
    );
    await para('Beta line').hover();
    await expect(handle).toBeVisible();
    const items = handle.locator('.operation-item');
    await items.first().hover();
    await expect(
      handle,
      'stays visible while hovering the handle',
    ).toBeVisible();
    await items.first().click();
    const menu = page.locator(
      '.page-editor .milkdown-slash-menu[data-show="true"]',
    );
    await expect(menu).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(menu).toBeHidden();
    await para('Gamma line').hover();
    await page.mouse.move(0, 0);
    await expect
      .poll(
        () =>
          page.evaluate(
            () =>
              getComputedStyle(
                document.querySelector('.page-editor .milkdown-block-handle'),
              ).visibility,
          ),
        { message: 'hidden once the pointer leaves the editor' },
      )
      .toBe('hidden');
  });

  test('block handle can be reached with a real pointer path from the text', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await setupPages(page, {
      richBody: 'Alpha line\n\nBeta line\n\nGamma line\n',
    });
    await openPage(page, 'rich-page', 1);
    const para = (text) =>
      page.locator('.page-editor .ProseMirror p', { hasText: text });
    const handle = page.locator(
      '.page-editor .milkdown-block-handle[data-show="true"]',
    );
    const centerY = async (locator) => {
      const box = await locator.boundingBox();
      return box.y + box.height / 2;
    };
    const gamma = await para('Gamma line').boundingBox();
    await page.mouse.move(gamma.x + 40, gamma.y + gamma.height / 2);
    await expect(handle).toBeVisible();
    const plus = await handle.locator('.operation-item').first().boundingBox();
    const betaY = await centerY(para('Beta line'));
    await page.mouse.move(plus.x + plus.width / 2, betaY, { steps: 40 });
    await expect
      .poll(async () => Math.abs((await centerY(handle)) - betaY), {
        message: 'handle follows the row the pointer is on in the gutter',
      })
      .toBeLessThan(4);
    await expect(handle).toHaveCSS('visibility', 'visible');
    await handle.locator('.operation-item').first().click();
    await expect(
      page.locator('.page-editor .milkdown-slash-menu[data-show="true"]'),
    ).toBeVisible();
  });

  test('block handle drag moves the block', async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    const state = await setupPages(page, {
      richBody: 'Alpha line\n\nBeta line\n\nGamma line\n',
    });
    await openPage(page, 'rich-page', 1);
    const para = (text) =>
      page.locator('.page-editor .ProseMirror p', { hasText: text });
    const handle = page.locator(
      '.page-editor .milkdown-block-handle[data-show="true"]',
    );
    await para('Alpha line').hover();
    await expect(handle).toBeVisible();
    const grip = handle.locator('.operation-item').last();
    await grip.hover();
    const g = await grip.boundingBox();
    const target = await para('Gamma line').boundingBox();
    await page.mouse.down();
    await page.mouse.move(g.x + g.width / 2 - 10, g.y + g.height / 2);
    await page.mouse.move(target.x + 10, target.y + target.height - 2, {
      steps: 5,
    });
    await page.mouse.up();
    await expect
      .poll(async () => (state.writes.length ? await savedBody(state) : ''), {
        timeout: 10_000,
      })
      .toBe('Beta line\n\nGamma line\n\nAlpha line\n');
  });

  test('tmgr chips show the current title on hover and navigate on click', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    const state = await setupPages(page, {
      richBody:
        'Task [Old name](tmgr://task/1) and page [Old page](tmgr://page/2).\n',
    });
    await openPage(page, 'rich-page', 1);
    const task = page.locator('.page-editor a.tmgr-chip-task');
    await expect(task).toHaveAttribute('data-current-title', 'Original task');
    await task.hover();
    expect(
      await task.evaluate((el) => getComputedStyle(el, '::after').content),
      'hover shows the current title',
    ).toBe('"Original task"');
    await page.locator('.page-editor a.tmgr-chip-page').click();
    await expect(page).toHaveURL(/\/demo\/pages\/empty-page$/);
    expect(state.writes, 'clicking a chip saves nothing').toEqual([]);
  });

  const mockUpload = async (page, { fail = false } = {}) => {
    const uploads = [];
    await page.route('**/api/files/presign-upload', (route) => {
      if (fail)
        return route.fulfill({ status: 422, json: { message: 'Too big' } });
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
      uploads.push(body.file_name);
      return route.fulfill({
        status: 201,
        json: {
          data: {
            id: 41,
            page_id: 1,
            name: body.file_name,
            mime_type: body.mime_type,
            size: body.size_bytes,
            created_at: '2026-10-01T10:00:00Z',
          },
        },
      });
    });
    return uploads;
  };

  const sendImage = (paragraph, kind) =>
    paragraph.evaluate(
      (el, { kind, bytes }) => {
        const data = new DataTransfer();
        data.items.add(
          new File([new Uint8Array(bytes)], 'shot.png', { type: 'image/png' }),
        );
        if (kind === 'paste') {
          el.dispatchEvent(
            new ClipboardEvent('paste', {
              clipboardData: data,
              bubbles: true,
              cancelable: true,
            }),
          );
          return;
        }
        const box = el.getBoundingClientRect();
        el.dispatchEvent(
          new DragEvent('drop', {
            dataTransfer: data,
            clientX: box.right - 2,
            clientY: box.top + box.height / 2,
            bubbles: true,
            cancelable: true,
          }),
        );
      },
      { kind, bytes: [...PNG] },
    );

  for (const kind of ['paste', 'drop']) {
    test(`image ${kind} uploads and saves a tmgr://file link`, async ({
      page,
    }) => {
      await page.setViewportSize(VIEWPORTS.desktop);
      const state = await setupPages(page, { richBody: 'Picture below\n' });
      const uploads = await mockUpload(page);
      await openPage(page, 'rich-page', 1);
      const line = page.locator('.page-editor .ProseMirror p', {
        hasText: 'Picture below',
      });
      await caretAtEnd(line);
      await sendImage(line, kind);
      await expect.poll(() => uploads).toEqual(['shot.png']);
      await expect
        .poll(async () =>
          /!\[\]\(tmgr:\/\/file\/41 " =\d+x\d+"\)/.test(await savedBody(state)),
        )
        .toBe(true);
      await expect(
        page.locator('.page-editor img.pe-image-img'),
      ).toHaveAttribute('src', /^blob:/);
    });
  }

  // The plan says "upload-error toast"; PageView shows it in the files panel (same as before this task).
  test('failed image upload reports the error and saves nothing', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    const state = await setupPages(page, { richBody: 'Picture below\n' });
    await mockUpload(page, { fail: true });
    await openPage(page, 'rich-page', 1);
    const line = page.locator('.page-editor .ProseMirror p', {
      hasText: 'Picture below',
    });
    await caretAtEnd(line);
    await sendImage(line, 'paste');
    await expect(page.getByText('Failed to upload shot.png')).toBeVisible();
    await expect(page.locator('.page-editor img')).toHaveCount(0);
    await page.waitForTimeout(DEBOUNCE_MARGIN);
    expect(state.writes).toEqual([]);
  });

  test('a page saved by the old editor (* bullets, *** rule) is not rewritten by opening and moving around', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    const body =
      '* First bullet\n* Second bullet\n\n***\n\n* [ ] Open item\n\nTail\n';
    const state = await setupPages(page, { richBody: body });
    await openPage(page, 'rich-page', 1);
    const first = page.locator('.page-editor .ProseMirror li', {
      hasText: 'First bullet',
    });
    await first.click();
    for (const key of ['ArrowDown', 'ArrowDown', 'ArrowUp', 'ArrowRight'])
      await page.keyboard.press(key);
    await page
      .locator('.page-editor .ProseMirror p', { hasText: 'Tail' })
      .hover();
    await page.waitForTimeout(DEBOUNCE_MARGIN);
    expect(state.writes).toEqual([]);
    await expect(page.getByTestId('page-save-status')).toHaveText(
      'Saved, version 3',
    );
  });

  test('the app theme switch (store setColorScheme) recolors the editor without a remount', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await useScheme(page, 'light');
    const state = await setupPages(page);
    await openPage(page, 'rich-page', 2);
    await expect(
      page.locator('.page-editor .tok-keyword').first(),
    ).toBeVisible();
    const read = () =>
      page.evaluate(() => {
        const editor = document.querySelector('.page-editor .milkdown');
        const code = document.querySelector(
          '.page-editor .milkdown-code-block',
        );
        const keyword = document.querySelector('.page-editor .tok-keyword');
        const section = document.querySelector(
          '[data-testid="managed-section"] li',
        );
        const sheets = [
          ...document.querySelectorAll('style, link[rel="stylesheet"]'),
        ];
        return {
          text: getComputedStyle(editor).color,
          code: getComputedStyle(code).backgroundColor,
          keyword: getComputedStyle(keyword).color,
          section: getComputedStyle(section).color,
          dark: document.documentElement.classList.contains('dark'),
          sheets: sheets.length,
          css: sheets.reduce(
            (sum, el) => sum + (el.textContent?.length ?? 0),
            0,
          ),
        };
      });
    const commit = (scheme) =>
      page.evaluate((value) => {
        const store =
          document.querySelector('#app').__vue_app__.config.globalProperties
            .$store;
        store.commit('setColorScheme', value);
      }, scheme);
    await page.evaluate(() => {
      window.__editorNode = document.querySelector('.page-editor .milkdown');
    });
    let settled = '';
    await expect
      .poll(
        async () => {
          const { sheets, css } = await read();
          const now = `${sheets}/${css}`;
          const same = now === settled;
          settled = now;
          return same;
        },
        { intervals: [500], message: 'stylesheets finished loading' },
      )
      .toBe(true);
    const light = await read();
    expect(light.dark).toBe(false);
    await commit('dark');
    await expect.poll(async () => (await read()).dark).toBe(true);
    const dark = await read();
    expect(dark.text).not.toBe(light.text);
    expect(dark.code).not.toBe(light.code);
    expect(dark.keyword).not.toBe(light.keyword);
    expect(dark.section).not.toBe(light.section);
    expect(
      { sheets: dark.sheets, css: dark.css },
      'no stylesheet swapping',
    ).toEqual({
      sheets: light.sheets,
      css: light.css,
    });
    await commit('default');
    await expect.poll(async () => (await read()).text).toBe(light.text);
    expect(
      await page.evaluate(
        () =>
          window.__editorNode ===
          document.querySelector('.page-editor .milkdown'),
      ),
      'same editor node after two switches',
    ).toBe(true);
    await page.waitForTimeout(DEBOUNCE_MARGIN);
    expect(state.writes, 'switching theme saves nothing').toEqual([]);
  });

  test('selection toolbar fits the phone viewport', async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.phone);
    await setupPages(page);
    await openPage(page, 'rich-page', 2);
    const intro = page.locator('.page-editor .ProseMirror p', {
      hasText: 'Intro paragraph',
    });
    for (const word of ['strong', 'code']) {
      await intro.locator(word).first().dblclick();
      const toolbar = page.locator(
        '.page-editor .milkdown-toolbar[data-show="true"]',
      );
      await expect(toolbar).toBeVisible();
      const box = await toolbar.boundingBox();
      expect(box.x, `${word}: left edge`).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width, `${word}: right edge`).toBeLessThanOrEqual(
        VIEWPORTS.phone.width,
      );
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth - window.innerWidth,
        ),
      ).toBeLessThanOrEqual(0);
    }
  });

  test('chip hover title does not overflow the phone viewport', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.phone);
    await setupPages(page, {
      richBody:
        'Some words to push the chip to the right edge [Old name](tmgr://task/1)\n',
    });
    await openPage(page, 'rich-page', 1);
    const chip = page.locator('.page-editor a.tmgr-chip-task');
    await expect(chip).toHaveAttribute('data-current-title', 'Original task');
    await chip.hover();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth - window.innerWidth,
      ),
    ).toBeLessThanOrEqual(0);
  });

  for (const [device, viewport] of Object.entries(VIEWPORTS)) {
    test(`title, editor text and managed section share one column and type (${device})`, async ({
      page,
    }) => {
      await page.setViewportSize(viewport);
      await setupPages(page);
      await openPage(page, 'rich-page', 2);
      const metrics = await page.evaluate(() => {
        const box = (el) => el.getBoundingClientRect();
        const type = (el) => {
          const s = getComputedStyle(el);
          return `${s.fontFamily}|${s.fontSize}|${s.lineHeight}`;
        };
        const title = document.querySelector('input[placeholder="Untitled"]');
        const para = [
          ...document.querySelectorAll('.page-editor .ProseMirror p'),
        ].find((p) => p.textContent.startsWith('Intro paragraph'));
        const sectionItem = document.querySelector(
          '[data-testid="managed-section"] li',
        );
        const editorItem = [
          ...document.querySelectorAll('.page-editor .ProseMirror li'),
        ].find((li) => li.textContent.includes('First bullet'));
        const card = document
          .querySelector('[data-testid="page-content"]')
          .closest('.rounded-card');
        return {
          titleLeft:
            box(title).left + parseFloat(getComputedStyle(title).paddingLeft),
          paraLeft: box(para).left,
          paraWidth: box(para).width,
          cardLeft: box(card).left,
          titleRatio:
            parseFloat(getComputedStyle(title).fontSize) /
            parseFloat(getComputedStyle(para).fontSize),
          editorItemType: type(editorItem.querySelector('p') ?? editorItem),
          sectionItemType: type(sectionItem.querySelector('p') ?? sectionItem),
        };
      });
      expect(
        Math.abs(metrics.titleLeft - metrics.paraLeft),
        'title aligned with body',
      ).toBeLessThanOrEqual(2);
      expect(
        metrics.titleRatio,
        'title reads as the document title',
      ).toBeGreaterThanOrEqual(1.6);
      expect(metrics.paraWidth, 'readable measure').toBeLessThanOrEqual(
        48 * 16,
      );
      expect(
        metrics.sectionItemType,
        'managed section uses the editor type',
      ).toBe(metrics.editorItemType);
      if (device === 'desktop') {
        const intro = page.locator('.page-editor .ProseMirror p', {
          hasText: 'Intro paragraph',
        });
        await intro.hover();
        const handle = page.locator(
          '.page-editor .milkdown-block-handle[data-show="true"]',
        );
        await expect(handle).toBeVisible();
        expect(
          (await handle.boundingBox()).x,
          'handle inside the card',
        ).toBeGreaterThanOrEqual(metrics.cardLeft);
      }
    });
  }
});

test('selecting text inside a code block still offers Make a task', async ({
  page,
}) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  await setupPages(page);
  await openPage(page, 'rich-page', 2);
  const line = page
    .locator('.page-editor .milkdown-code-block .cm-line')
    .first();
  await line.scrollIntoViewIfNeeded();
  await line.locator('text=answer').first().dblclick();
  const toolbarItem = page.locator(
    '.page-editor .milkdown-toolbar[data-show="true"] [data-toolbar-item="make-task"]',
  );
  const button = page.getByTestId('selection-to-task');
  await expect(toolbarItem.or(button)).toBeVisible();
  await button.click();
  const dialog = page.getByRole('dialog', { name: 'Make a task' });
  await expect(dialog.locator('blockquote')).toHaveText('answer');
});

test('resolving a conflict remounts the editors without hiding the page', async ({
  page,
}) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  await page.addInitScript(() => {
    window.__hiddenWhileArmed = false;
    const sample = () => {
      const doc = document.querySelector('.page-doc');
      if (
        window.__armed &&
        doc &&
        getComputedStyle(doc).visibility === 'hidden'
      ) {
        window.__hiddenWhileArmed = true;
      }
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  });
  await setupPages(page);
  await page.route('**/api/pages/1', (route) => {
    if (route.request().method() === 'GET') return route.fallback();
    return route.fulfill({
      status: 409,
      json: {
        error: 'page_conflict',
        data: {
          id: 1,
          title: 'Rich page',
          slug: 'rich-page',
          type: 'plain',
          parent_id: null,
          position: 1,
          pinned: false,
          updated_at: '2026-10-02T10:00:00Z',
          workspace_id: 1,
          body: `${RICH_BODY}Remote line.\n`,
          properties: {},
          version: 9,
          author,
          updated_by: author,
          created_at: '2026-09-30T10:00:00Z',
          backlinks: [],
          sections: [],
          files: [],
          following: false,
        },
      },
    });
  });
  await openPage(page, 'rich-page', 2);
  await page.evaluate(() => {
    window.__armed = true;
  });
  await caretAtEnd(
    page.locator('.page-editor .ProseMirror p', {
      hasText: 'Closing paragraph',
    }),
  );
  await page.keyboard.type('!');
  const theirs = page.getByTestId('conflict-theirs-btn');
  const resolve = page.getByRole('button', { name: 'Resolve', exact: true });
  await expect(theirs.or(resolve).first()).toBeVisible();
  if (!(await theirs.isVisible())) await resolve.click();
  await theirs.click();
  await expect(
    page.locator('.page-editor .ProseMirror p', { hasText: 'Remote line.' }),
  ).toBeVisible();
  await page.waitForTimeout(300);
  expect(await page.evaluate(() => window.__hiddenWhileArmed)).toBe(false);
});

test('task lists: a checked parent leaves sub-items alone, loose lists get no extra bullet', async ({
  page,
}) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  const body = [
    '- [x] Parent done',
    '  - [ ] Child open',
    '',
    '<!-- tmgr:section id="t" owner="agents" -->',
    '- [ ] Loose one',
    '',
    '- [x] Loose two',
    '  - [ ] Loose child',
    '<!-- /tmgr:section -->',
    '',
  ].join('\n');
  await setupPages(page, { richBody: body });
  await openPage(page, 'rich-page', 2);
  const decoration = (locator) =>
    locator.evaluate((el) => {
      let line = 'none';
      for (let n = el; n && line === 'none'; n = n.parentElement)
        line = getComputedStyle(n).textDecorationLine;
      return line;
    });
  const editor = page.locator('.page-editor .ProseMirror');
  await expect
    .poll(() => decoration(editor.locator('p', { hasText: 'Parent done' })))
    .toBe('line-through');
  expect(await decoration(editor.locator('p', { hasText: 'Child open' }))).toBe(
    'none',
  );
  const loose = page
    .getByTestId('managed-section')
    .locator('li', { hasText: 'Loose one' });
  expect(
    await loose.evaluate((el) => getComputedStyle(el, '::before').content),
  ).toBe('none');
  expect(
    await loose
      .locator('input[type="checkbox"]')
      .evaluate((el) => getComputedStyle(el).appearance),
  ).toBe('none');
  const color = (text) =>
    page
      .getByTestId('managed-section')
      .locator('li', { hasText: text })
      .last()
      .evaluate((el) => getComputedStyle(el).color);
  expect(await color('Loose child'), 'sub-item keeps the text colour').toBe(
    await color('Loose one'),
  );
});

const coveredFirstCodeLine = (page) =>
  page
    .locator('.page-editor .milkdown-code-block')
    .first()
    .evaluate((block) => {
      const range = document.createRange();
      range.selectNodeContents(block.querySelector('.cm-line'));
      const text = range.getBoundingClientRect();
      return [
        ...block.querySelectorAll(
          '.tools .language-button, .tools .tools-button-group button',
        ),
      ]
        .filter((el) => {
          const r = el.getBoundingClientRect();
          return (
            r.width &&
            Number(getComputedStyle(el).opacity) !== 0 &&
            r.left < text.right &&
            r.right > text.left &&
            r.top < text.bottom &&
            r.bottom > text.top
          );
        })
        .map((el) => el.textContent.trim() || el.className);
    });

for (const touch of [false, true]) {
  test.describe(`code block tools on a ${
    touch ? 'touch' : 'mouse'
  } phone`, () => {
    test.use({ viewport: VIEWPORTS.phone, hasTouch: touch, isMobile: touch });

    test(`phone 390 ${
      touch ? 'touch' : 'mouse'
    }: code block tools do not cover the first code line`, async ({ page }) => {
      await setupPages(page);
      await openPage(page, 'rich-page', 2);
      const line = page
        .locator('.page-editor .milkdown-code-block .cm-line')
        .first();
      await line.scrollIntoViewIfNeeded();
      expect(await coveredFirstCodeLine(page), 'idle').toEqual([]);
      if (touch) await line.tap();
      else await line.click();
      await page.waitForTimeout(400);
      expect(await coveredFirstCodeLine(page), 'editing').toEqual([]);
    });
  });
}
