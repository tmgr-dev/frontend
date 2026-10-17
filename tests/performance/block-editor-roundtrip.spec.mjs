import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test.setTimeout(90_000);

const YOUTUBE_URL = 'https://www.youtube.com/watch?v=dQw4w9WgXcQ';

const baseTask = {
  title: 'Block task',
  description: null,
  description_json: null,
  status: 'created',
  status_id: 1,
  workspace_id: 1,
  user_id: 1,
  assignees: [],
  settings: [],
  checkpoints: [],
  common_time: 0,
  start_time: 0,
  approximately_time: 0,
  project_category_id: null,
  created_at: '2026-09-13T10:00:00Z',
  updated_at: '2026-09-13T10:00:00Z',
};

const setup = async (page, { task, scheme } = {}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(`pageerror: ${error.message}`));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(`console: ${message.text()}`);
  });
  await page.addInitScript((scheme) => {
    localStorage.setItem('preferred_editor', 'block');
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    if (scheme) localStorage.setItem('colorScheme', scheme);
  }, scheme);
  await mockApp(page);
  await page.route(
    /^https:\/\/(www\.youtube\.com|cdn\.jsdelivr\.net)\//,
    (route) =>
      route.fulfill({ status: 200, contentType: 'text/html', body: '' }),
  );
  const created = [];
  await page.route('**/api/tasks', async (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    const body = route.request().postDataJSON();
    created.push(body);
    await route.fulfill({
      json: { data: { ...baseTask, ...body, id: 2, title: body.title } },
    });
  });
  // Every non-GET request to the opened task, so a write by any method is counted.
  const writes = [];
  if (task)
    await page.route('**/api/tasks/2', async (route) => {
      const method = route.request().method();
      if (method === 'GET')
        return route.fulfill({
          json: { data: { ...baseTask, id: 2, ...task() } },
        });
      const body = route.request().postDataJSON();
      writes.push({ method, body });
      await route.fulfill({
        json: { data: { ...baseTask, id: 2, ...task(), ...body } },
      });
    });
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  return { errors, created, writes };
};

const openTask = (page, id) =>
  page.evaluate(
    (id) =>
      document
        .querySelector('#app')
        .__vue_app__.config.globalProperties.$router.push(`/demo/tasks/${id}`),
    id,
  );

const editorErrors = (errors) =>
  errors.filter((line) =>
    /editor|codex|block|embed|table|header|quote|paste/i.test(line),
  );

// @editorjs/list@2 puts three toolbox entries under one data-item-name; `title` picks one.
const toolboxItem = (page, item, title) => {
  const option = page.locator(`.ce-popover--opened [data-item-name="${item}"]`);
  return title
    ? option.filter({
        has: page.locator('.ce-popover-item__title', {
          hasText: new RegExp(`^${title}$`),
        }),
      })
    : option.first();
};

const openToolbox = async (page, item, title) => {
  await page.keyboard.type('/');
  await toolboxItem(page, item, title).click();
};

const addBlock = async (page, item, title) => {
  const blocks = page.locator('.new-form-container .codex-editor .ce-block');
  // Typing hides the toolbar; it reappears when the hovered block changes.
  await blocks.first().hover();
  await blocks.last().hover();
  await page.locator('.new-form-container .ce-toolbar__plus').click();
  await toolboxItem(page, item, title).click();
};

test('header, quote, table, embed and paragraph blocks survive create and reopen', async ({
  page,
}) => {
  let saved;
  const { errors, created } = await setup(page, {
    task: () => ({ description_json: saved }),
  });
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  const editor = page.locator('.new-form-container .codex-editor');
  await editor.locator('.ce-paragraph').first().waitFor();

  // Header via the slash toolbox.
  await editor.locator('.ce-paragraph').first().click();
  await openToolbox(page, 'header');
  await page.keyboard.type('Roundtrip header');

  // Plus button, not Enter: Enter right after a toolbox conversion adds a line break (see the skipped test below).
  await addBlock(page, 'paragraph');
  await page.keyboard.type('Plain paragraph text');

  await addBlock(page, 'quote');
  await page.keyboard.type('Roundtrip quote');
  await editor.locator('.cdx-quote__caption').click();
  await page.keyboard.type('Quote author');

  // Table via the plus button toolbox.
  await addBlock(page, 'table');
  const cells = editor.locator('.tc-table .tc-cell');
  await expect(cells).toHaveCount(6);
  await cells.nth(0).click();
  await page.keyboard.type('Cell A1');
  await cells.nth(4).click();
  await page.keyboard.type('Cell B2');

  // Embed via a paste of a YouTube link into a fresh paragraph.
  await addBlock(page, 'paragraph');
  await page.evaluate((url) => {
    const target = document.activeElement;
    const data = new DataTransfer();
    data.setData('text/plain', url);
    target.dispatchEvent(
      new ClipboardEvent('paste', {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, YOUTUBE_URL);
  await expect(editor.locator('.embed-tool iframe')).toHaveAttribute(
    'src',
    /youtube\.com\/embed\/dQw4w9WgXcQ/,
  );

  await page.locator('.new-form-container button[title="Create"]').click();
  await expect.poll(() => created.length).toBe(1);

  const body = created[0];
  expect(body.description).toBeNull();
  const blocks = body.description_json?.blocks ?? [];
  const byType = (type) => blocks.filter((block) => block.type === type);
  expect(byType('header')[0]?.data.text).toBe('Roundtrip header');
  expect(byType('paragraph').map((block) => block.data.text)).toContain(
    'Plain paragraph text',
  );
  expect(byType('quote')[0]?.data).toMatchObject({
    text: 'Roundtrip quote',
    caption: 'Quote author',
  });
  expect(byType('table')[0]?.data.content).toEqual([
    ['Cell A1', '', ''],
    ['', 'Cell B2', ''],
  ]);
  expect(byType('embed')[0]?.data).toMatchObject({
    service: 'youtube',
    source: YOUTUBE_URL,
    embed: 'https://www.youtube.com/embed/dQw4w9WgXcQ',
  });
  saved = body.description_json;

  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await openTask(page, 2);
  await page.getByRole('button', { name: 'Edit description' }).click();
  const reopened = page.locator('.codex-editor');
  await expect(reopened.locator('.ce-header')).toHaveText('Roundtrip header');
  await expect(
    reopened.locator('.ce-paragraph', { hasText: 'Plain paragraph text' }),
  ).toBeVisible();
  await expect(reopened.locator('.cdx-quote__text')).toHaveText(
    'Roundtrip quote',
  );
  await expect(reopened.locator('.cdx-quote__caption')).toHaveText(
    'Quote author',
  );
  await expect(reopened.locator('.tc-cell').nth(0)).toHaveText('Cell A1');
  await expect(reopened.locator('.tc-cell').nth(4)).toHaveText('Cell B2');
  await expect(reopened.locator('.embed-tool iframe')).toHaveAttribute(
    'src',
    /youtube\.com\/embed\/dQw4w9WgXcQ/,
  );
  expect(errors.filter((line) => line.startsWith('pageerror'))).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

test('empty block description opens an empty editor without errors', async ({
  page,
}) => {
  const { errors } = await setup(page, {
    task: () => ({ description: null, description_json: null }),
  });
  await openTask(page, 2);
  await page.getByText('No description — click to add').click();
  const editor = page.locator('.codex-editor');
  await expect(editor.locator('.ce-paragraph')).toHaveCount(1);
  await expect(editor.locator('.ce-paragraph')).toHaveText('');
  expect(errors.filter((line) => line.startsWith('pageerror'))).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

test('legacy HTML string description renders through renderFromHTML', async ({
  page,
}) => {
  const { errors } = await setup(page, {
    task: () => ({
      description: null,
      description_json: '<p>Legacy HTML paragraph</p><p>Second legacy line</p>',
    }),
  });
  await openTask(page, 2);
  await page.getByRole('button', { name: 'Edit description' }).click();
  const editor = page.locator('.codex-editor');
  await expect(editor).toContainText('Legacy HTML paragraph');
  await expect(editor).toContainText('Second legacy line');
  expect(errors.filter((line) => line.startsWith('pageerror'))).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

test('Create right after the last keystroke keeps that text in the request', async ({
  page,
}) => {
  const { created } = await setup(page);
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  const paragraph = page
    .locator('.new-form-container .codex-editor .ce-paragraph')
    .first();
  await paragraph.click();
  await page.keyboard.type('Typed just before Create');
  await page.locator('.new-form-container button[title="Create"]').click();
  await expect.poll(() => created.length).toBe(1);
  expect(
    created[0].description_json?.blocks?.map((block) => block.data.text),
  ).toContain('Typed just before Create');
});

test('Create right after the last keystroke in Block MD keeps that text in the request', async ({
  page,
}) => {
  const { created } = await setup(page);
  await page.evaluate(() => {
    localStorage.setItem('preferred_editor', 'blockmd');
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      );
  });
  await page.locator('.new-form-container .milkdown .ProseMirror').click();
  await page.keyboard.type('Typed just before Create');
  await page.locator('.new-form-container button[title="Create"]').click();
  await expect.poll(() => created.length).toBe(1);
  expect(created[0].description).toContain('Typed just before Create');
});

test('Enter after picking Heading from the toolbox starts a new block', async ({
  page,
}) => {
  test.skip(
    !process.env.RUN_KNOWN_BUGS,
    'Known bug: Enter in a block just converted via the toolbox inserts a line break inside the header instead of a new block',
  );
  await setup(page);
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  const editor = page.locator('.new-form-container .codex-editor');
  await editor.locator('.ce-paragraph').first().click();
  await openToolbox(page, 'header');
  await page.keyboard.type('Heading');
  await page.keyboard.press('Enter');
  await page.keyboard.type('Next line');
  await expect(editor.locator('.ce-block')).toHaveCount(2);
  await expect(editor.locator('.ce-header')).toHaveText('Heading');
});

// TM-582: description_json stored by list@1 and the old checklist tool.
const legacyListDoc = () => ({
  time: 1,
  version: '2.30.6',
  blocks: [
    { id: 'p1', type: 'paragraph', data: { text: 'Intro' } },
    {
      id: 'l1',
      type: 'list',
      data: { style: 'unordered', items: ['alpha', 'beta'] },
    },
    {
      id: 'l2',
      type: 'list',
      data: { style: 'ordered', items: ['one', 'two'] },
    },
    {
      id: 'l3',
      type: 'list',
      data: {
        style: 'unordered',
        items: [
          { content: 'parent', items: [{ content: 'child', items: [] }] },
        ],
      },
    },
    {
      id: 'c1',
      type: 'checklist',
      data: {
        items: [
          { text: 'done', checked: true },
          { text: 'todo', checked: false },
        ],
      },
    },
  ],
});

const listItem = (content, meta = {}, items = []) => ({ content, meta, items });

const SCREENSHOTS = process.env.QA_SCREENSHOTS_DIR;

const pageErrors = (errors) =>
  errors.filter((line) => line.startsWith('pageerror'));

const caretAtEnd = (locator) =>
  locator.evaluate((element) => {
    element.focus();
    const range = document.createRange();
    range.selectNodeContents(element);
    range.collapse(false);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
  });

// Leaving the task route unmounts NewForm, whose onBeforeUnmount flushes the editor.
const closeTask = async (page) => {
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$router.push('/'),
  );
  await expect(page.locator('.new-form-container')).toHaveCount(0);
};

const openEditor = async (page) => {
  await page.getByRole('button', { name: 'Edit description' }).click();
  return page.locator('.new-form-container .codex-editor');
};

test('legacy list, nested list and checklist open without a stub and save as list@2', async ({
  page,
}) => {
  let saved = legacyListDoc();
  const { errors, writes } = await setup(page, {
    task: () => ({ description_json: saved }),
  });
  await openTask(page, 2);
  const preview = page.locator('.description-editor-wrapper');
  await expect(
    preview.locator('ul.checklist input[type="checkbox"]'),
  ).toHaveCount(2);
  const editor = await openEditor(page);
  await expect(
    editor.locator('.cdx-list__item-content', { hasText: 'child' }),
  ).toBeVisible();
  await expect(editor.locator('.ce-stub')).toHaveCount(0);
  await expect(editor.locator('.cdx-list-ordered')).toHaveCount(1);
  await expect(
    editor.locator('.cdx-list-checklist .cdx-list__checkbox--checked'),
  ).toHaveCount(1);

  await editor
    .locator('.cdx-list-checklist .cdx-list__item', { hasText: 'todo' })
    .locator('.cdx-list__checkbox')
    .first()
    .click();
  const alpha = editor.locator('.cdx-list__item-content', { hasText: 'alpha' });
  await alpha.click();
  await caretAtEnd(alpha);
  await page.keyboard.type('X');
  await page.waitForTimeout(1000);
  await closeTask(page);
  await expect.poll(() => writes.length, { timeout: 8000 }).toBeGreaterThan(0);

  const last = writes.at(-1);
  expect(last.method).toBe('PUT');
  const blocks = last.body.description_json.blocks;
  expect(blocks.map((block) => block.type)).toEqual([
    'paragraph',
    'list',
    'list',
    'list',
    'list',
  ]);
  expect(blocks.find((block) => block.id === 'l1').data).toEqual({
    style: 'unordered',
    meta: {},
    items: [listItem('alphaX'), listItem('beta')],
  });
  expect(blocks.find((block) => block.id === 'l3').data.items).toEqual([
    listItem('parent', {}, [listItem('child')]),
  ]);
  expect(blocks.find((block) => block.id === 'c1').data).toEqual({
    style: 'checklist',
    meta: {},
    items: [
      listItem('done', { checked: true }),
      listItem('todo', { checked: true }),
    ],
  });
  saved = last.body.description_json;

  await openTask(page, 2);
  const reopened = await openEditor(page);
  await expect(
    reopened.locator('.cdx-list__item-content', { hasText: 'alphaX' }),
  ).toBeVisible();
  await expect(
    reopened.locator('.cdx-list-checklist .cdx-list__checkbox--checked'),
  ).toHaveCount(2);
  await expect(reopened.locator('.ce-stub')).toHaveCount(0);
  expect(pageErrors(errors)).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

test('opening and closing a legacy list task without edits sends no write', async ({
  page,
}) => {
  const { errors, writes } = await setup(page, {
    task: () => ({ description_json: legacyListDoc() }),
  });
  await openTask(page, 2);
  const editor = await openEditor(page);
  await expect(
    editor.locator('.cdx-list__item-content', { hasText: 'todo' }),
  ).toBeVisible();
  // Longer than the 400ms Editor.js onChange batch.
  await page.waitForTimeout(1500);
  await closeTask(page);
  await page.waitForTimeout(2000);
  expect(writes).toEqual([]);
  expect(pageErrors(errors)).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

test('lists created via slash menu and plus button are posted as list@2 and reopen without a write', async ({
  page,
}) => {
  let saved;
  const { errors, created, writes } = await setup(page, {
    task: () => ({ description_json: saved }),
  });
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  const editor = page.locator('.new-form-container .codex-editor');
  await editor.locator('.ce-paragraph').first().click();
  await openToolbox(page, 'list', 'Unordered List');
  await page.keyboard.type('bullet one');
  await page.keyboard.press('Enter');
  await page.keyboard.press('Tab');
  await page.keyboard.type('bullet nested');
  // Enter on an empty nested item outdents it, on an empty top-level item leaves the list.
  await page.keyboard.press('Enter');
  await page.keyboard.press('Enter');
  await page.keyboard.press('Enter');
  await expect(editor.locator('.ce-block')).toHaveCount(2);

  await addBlock(page, 'list', 'Ordered List');
  await page.keyboard.type('number one');

  await addBlock(page, 'list', 'Checklist');
  await page.keyboard.type('check me');
  await editor
    .locator('.cdx-list-checklist .cdx-list__checkbox')
    .first()
    .click();

  // Editor.js batches onChange for 400ms and Create does not flush the editor (TM-584).
  await page.waitForTimeout(1000);
  await page.locator('.new-form-container button[title="Create"]').click();
  await expect.poll(() => created.length).toBe(1);

  const lists = created[0].description_json.blocks.filter(
    (block) => block.type === 'list',
  );
  expect(lists.map((block) => block.data.style)).toEqual([
    'unordered',
    'ordered',
    'checklist',
  ]);
  expect(lists[0].data.items).toEqual([
    listItem('bullet one', {}, [listItem('bullet nested')]),
  ]);
  expect(lists[1].data.items).toEqual([listItem('number one')]);
  expect(lists[2].data.items).toEqual([
    listItem('check me', { checked: true }),
  ]);
  saved = created[0].description_json;

  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await openTask(page, 2);
  const reopened = await openEditor(page);
  await expect(
    reopened.locator('.cdx-list__item-content', { hasText: 'bullet nested' }),
  ).toBeVisible();
  await expect(reopened.locator('.cdx-list-ordered')).toHaveCount(1);
  await expect(
    reopened.locator('.cdx-list-checklist .cdx-list__checkbox--checked'),
  ).toHaveCount(1);
  // Data saved by list@2 itself must also compare equal on close (D-003).
  await page.waitForTimeout(1500);
  await closeTask(page);
  await page.waitForTimeout(2000);
  expect(writes).toEqual([]);
  expect(pageErrors(errors)).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

test('empty and malformed list blocks load without errors and render nothing', async ({
  page,
}) => {
  const { errors, writes } = await setup(page, {
    task: () => ({
      description_json: {
        blocks: [
          { id: 'e1', type: 'list', data: { style: 'unordered', items: [] } },
          { id: 'e2', type: 'checklist', data: { items: [] } },
          { id: 'e3', type: 'checklist', data: { items: [null, 3] } },
          { id: 'e4', type: 'list', data: {} },
          { id: 'p', type: 'paragraph', data: { text: 'only' } },
        ],
      },
    }),
  });
  await openTask(page, 2);
  const preview = page.locator('.description-editor-wrapper');
  await expect(preview).toContainText('only');
  await expect(preview.locator('li')).toHaveCount(0);
  const editor = await openEditor(page);
  await expect(
    editor.locator('.ce-paragraph', { hasText: 'only' }),
  ).toBeVisible();
  await expect(editor.locator('.cdx-list')).toHaveCount(0);
  await expect(editor.locator('.ce-stub')).toHaveCount(0);
  await page.waitForTimeout(1500);
  await closeTask(page);
  await page.waitForTimeout(2000);
  expect(writes).toEqual([]);
  expect(pageErrors(errors)).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

for (const scheme of ['light', 'dark'])
  test(`list and checklist items are aligned in ${scheme} mode`, async ({
    page,
  }) => {
    const doc = legacyListDoc();
    doc.blocks.push({
      id: 'n1',
      type: 'list',
      data: {
        style: 'checklist',
        meta: {},
        items: [
          listItem('parent done', { checked: true }, [
            listItem('child todo', { checked: false }),
          ]),
        ],
      },
    });
    const { errors } = await setup(page, {
      scheme: scheme === 'dark' ? 'dark' : 'default',
      task: () => ({ description_json: doc }),
    });
    await openTask(page, 2);
    const editor = await openEditor(page);
    await expect(
      editor.locator('.cdx-list__item-content', { hasText: 'child todo' }),
    ).toBeVisible();
    expect(
      await page.evaluate(() =>
        document.documentElement.classList.contains('dark'),
      ),
    ).toBe(scheme === 'dark');

    // Every top-level marker starts at one x; text follows its marker without a wide gap.
    const rows = await editor
      .locator('.cdx-list__item')
      .evaluateAll((elements) =>
        elements
          .filter(
            (element) => !element.parentElement.closest('.cdx-list__item'),
          )
          .map((element) => {
            const content = element.querySelector(
              ':scope > .cdx-list__item-content',
            );
            return [
              Math.round(element.getBoundingClientRect().left),
              Math.round(content.getBoundingClientRect().left),
            ];
          }),
      );
    expect(rows).toHaveLength(8);
    expect(new Set(rows.map(([item]) => item)).size).toBe(1);
    for (const [item, content] of rows)
      expect(content - item).toBeLessThanOrEqual(24);
    const nested = editor.locator('.cdx-list__item-content', {
      hasText: 'child todo',
    });
    const parent = editor.locator('.cdx-list__item-content', {
      hasText: 'parent done',
    });
    expect((await nested.boundingBox()).x).toBeGreaterThan(
      (await parent.boundingBox()).x,
    );
    for (const item of await editor
      .locator('.cdx-list-checklist .cdx-list__item')
      .all()) {
      const box = await item
        .locator('.cdx-list__checkbox')
        .first()
        .boundingBox();
      const text = await item
        .locator('.cdx-list__item-content')
        .first()
        .boundingBox();
      expect(box.width).toBeGreaterThan(0);
      expect(box.x + box.width).toBeLessThanOrEqual(text.x);
    }
    if (SCREENSHOTS) {
      await page.waitForTimeout(500);
      await editor.screenshot({
        path: `${SCREENSHOTS}/lists-editor-${scheme}.png`,
      });
      await page.locator('.new-form-container').screenshot({
        path: `${SCREENSHOTS}/lists-form-${scheme}.png`,
      });
    }
    expect(pageErrors(errors)).toEqual([]);
    expect(editorErrors(errors)).toEqual([]);
  });

// Unlike header (TM-585), list@2 handles Enter itself right after a toolbox conversion.
test('Enter right after picking a list from the toolbox adds a second item', async ({
  page,
}) => {
  await setup(page);
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  const editor = page.locator('.new-form-container .codex-editor');
  await editor.locator('.ce-paragraph').first().click();
  await openToolbox(page, 'list', 'Unordered List');
  await page.keyboard.type('first');
  await page.keyboard.press('Enter');
  await page.keyboard.type('second');
  await expect(editor.locator('.cdx-list__item')).toHaveCount(2);
  await expect(editor.locator('.cdx-list__item-content').nth(1)).toHaveText(
    'second',
  );
});

for (const scheme of ['light', 'dark'])
  test(`ordered list text starts at one x for one- and two-digit numbers in ${scheme} mode`, async ({
    page,
  }) => {
    const items = Array.from({ length: 11 }, (_, index) =>
      listItem(`item ${index + 1}`),
    );
    items[9] = listItem('item 10', {}, [
      listItem('nested 1'),
      listItem('nested 2', {}, [listItem('deep 1')]),
    ]);
    await setup(page, {
      scheme: scheme === 'dark' ? 'dark' : 'default',
      task: () => ({
        description_json: {
          blocks: [
            {
              id: 'o',
              type: 'list',
              data: { style: 'ordered', meta: {}, items },
            },
          ],
        },
      }),
    });
    await openTask(page, 2);
    const editor = await openEditor(page);
    await expect(editor.locator('.cdx-list__item-content')).toHaveCount(14);
    // [depth, text left, marker left, marker right, editor left]
    const rows = await editor
      .locator('.cdx-list__item')
      .evaluateAll((elements) =>
        elements.map((element) => {
          let depth = 0;
          for (
            let parent = element.parentElement.closest('.cdx-list__item');
            parent;
            parent = parent.parentElement.closest('.cdx-list__item')
          )
            depth += 1;
          const item = element.getBoundingClientRect();
          const marker = parseFloat(
            getComputedStyle(element, '::before').width,
          );
          const content = element
            .querySelector(':scope > .cdx-list__item-content')
            .getBoundingClientRect();
          return [
            depth,
            Math.round(content.left),
            Math.round(item.left - marker),
            Math.round(item.left),
            Math.round(
              element.closest('.codex-editor').getBoundingClientRect().left,
            ),
          ];
        }),
      );
    for (const depth of [0, 1, 2]) {
      const level = rows.filter((row) => row[0] === depth);
      expect(level.length).toBeGreaterThan(0);
      expect(new Set(level.map((row) => row[1])).size).toBe(1);
      // Hanging marker: right edges line up, so numbers are right-aligned.
      expect(new Set(level.map((row) => row[3])).size).toBe(1);
    }
    for (const [depth, text, markerLeft, , editorLeft] of rows) {
      expect(markerLeft).toBeGreaterThanOrEqual(editorLeft);
      if (depth) expect(text).toBeGreaterThan(rows[0][1]);
    }
    if (SCREENSHOTS) {
      await page.waitForTimeout(500);
      await editor.screenshot({
        path: `${SCREENSHOTS}/ordered-11-${scheme}.png`,
      });
    }
  });
