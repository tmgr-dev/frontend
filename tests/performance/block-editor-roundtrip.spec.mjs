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
  const preview = page.locator('.description-editor-wrapper');
  await expect(preview).toContainText('Legacy HTML paragraph');
  await expect(preview).toContainText('Second legacy line');
  await expect(preview.getByText('No description — click to add')).toHaveCount(
    0,
  );
  await page.getByRole('button', { name: 'Edit description' }).click();
  const editor = page.locator('.codex-editor');
  await expect(editor).toContainText('Legacy HTML paragraph');
  await expect(editor).toContainText('Second legacy line');
  expect(errors.filter((line) => line.startsWith('pageerror'))).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

test('legacy HTML preview runs no script or event handler from the description', async ({
  page,
}) => {
  await setup(page, {
    task: () => ({
      description: null,
      description_json:
        '<p>Safe legacy text</p><img src="x" onerror="window.__tm586=1"><script>window.__tm586=2</script>',
    }),
  });
  await openTask(page, 2);
  const preview = page.locator('.description-editor-wrapper');
  await expect(preview).toContainText('Safe legacy text');
  await expect(preview.locator('img')).toHaveCount(1);
  await expect(preview.locator('script, [onerror]')).toHaveCount(0);
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

// TM-585: holds back Editor.js's deferred block event binding 3 s and counts pending binds, so a
// test can prove an Enter landed before the bind (pending > 0) or after it (pending 0).
const trackBlockEventBinding = (page) =>
  page.addInitScript(() => {
    const original = window.requestIdleCallback;
    window.__pendingBlockBinds = 0;
    window.requestIdleCallback = (callback, options) => {
      if (!String(callback).includes('bindBlockEvents'))
        return original(callback, options);
      window.__pendingBlockBinds += 1;
      const bind = (...args) => {
        window.__pendingBlockBinds -= 1;
        return callback(...args);
      };
      return setTimeout(() => original(bind, options), 3000);
    };
  });

const pendingBinds = (page) => page.evaluate(() => window.__pendingBlockBinds);

const pressUnboundEnter = async (page) => {
  expect(await pendingBinds(page)).toBeGreaterThan(0);
  await page.keyboard.press('Enter');
};

const paragraphHtml = (editor) =>
  editor
    .locator('.ce-paragraph')
    .evaluateAll((nodes) => nodes.map((node) => node.innerHTML));

const typeInNewTask = async (page) => {
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
  await expect.poll(() => pendingBinds(page)).toBe(0);
  await editor.locator('.ce-paragraph').first().click();
  return editor;
};

const createTask = async (page, created) => {
  await page.locator('.new-form-container button[title="Create"]').click();
  await expect.poll(() => created.length).toBe(1);
  return created[0].description_json.blocks.map((block) => [
    block.type,
    block.data.text,
  ]);
};

test('Enter after picking Heading from the toolbox starts a new block', async ({
  page,
}) => {
  await trackBlockEventBinding(page);
  await setup(page);
  const editor = await typeInNewTask(page);
  await openToolbox(page, 'header');
  await page.keyboard.type('Heading');
  await pressUnboundEnter(page);
  await page.keyboard.type('Next line');
  await expect(editor.locator('.ce-block')).toHaveCount(2);
  await expect(editor.locator('.ce-header')).toHaveText('Heading');
  await expect(editor.locator('.ce-paragraph')).toHaveText('Next line');
});

test('Shift+Enter in a just-created paragraph keeps a line break in it', async ({
  page,
}) => {
  await trackBlockEventBinding(page);
  await setup(page);
  const editor = await typeInNewTask(page);
  await page.keyboard.type('a');
  await page.keyboard.press('Enter');
  await page.keyboard.type('b');
  expect(await pendingBinds(page)).toBeGreaterThan(0);
  await page.keyboard.press('Shift+Enter');
  await page.keyboard.type('c');
  await expect(editor.locator('.ce-block')).toHaveCount(2);
  expect((await paragraphHtml(editor))[1]).toMatch(/^b<br>c/);
});

test('Enter in a just-created quote stays inside the quote', async ({
  page,
}) => {
  await trackBlockEventBinding(page);
  await setup(page);
  const editor = await typeInNewTask(page);
  await openToolbox(page, 'quote');
  await page.keyboard.type('first');
  await pressUnboundEnter(page);
  await page.keyboard.type('second');
  await expect(editor.locator('.ce-block')).toHaveCount(1);
  await expect(editor.locator('.cdx-quote')).toHaveCount(1);
  expect(
    await editor.locator('.cdx-quote__text').evaluate((node) => node.innerText),
  ).toMatch(/^first\n+second$/);
});

test('Enter in the middle of a just-created heading splits it', async ({
  page,
}) => {
  await trackBlockEventBinding(page);
  await setup(page);
  const editor = await typeInNewTask(page);
  await openToolbox(page, 'header');
  await page.keyboard.type('HeadTail');
  for (let i = 0; i < 4; i += 1) await page.keyboard.press('ArrowLeft');
  await pressUnboundEnter(page);
  await page.keyboard.type('X');
  await expect(editor.locator('.ce-block')).toHaveCount(2);
  await expect(editor.locator('.ce-header')).toHaveText('Head');
  await expect(editor.locator('.ce-paragraph')).toHaveText('XTail');
});

test('fast Enter in the unbound window posts three paragraphs on Create', async ({
  page,
}) => {
  await trackBlockEventBinding(page);
  const { created, errors } = await setup(page);
  const editor = await typeInNewTask(page);
  await page.keyboard.type('a');
  await page.keyboard.press('Enter');
  await page.keyboard.type('b');
  await pressUnboundEnter(page);
  await page.keyboard.type('c');
  await expect(editor.locator('.ce-paragraph')).toHaveText(['a', 'b', 'c']);
  expect(await createTask(page, created)).toEqual([
    ['paragraph', 'a'],
    ['paragraph', 'b'],
    ['paragraph', 'c'],
  ]);
  expect(pageErrors(errors)).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

test('Enter in blocks whose handlers are proven bound adds one block each', async ({
  page,
}) => {
  await trackBlockEventBinding(page);
  await setup(page);
  const editor = await typeInNewTask(page);
  await page.keyboard.type('one');
  await page.keyboard.press('Enter');
  await page.keyboard.type('two');
  await expect.poll(() => pendingBinds(page)).toBe(0);
  await page.keyboard.press('Enter');
  await page.keyboard.type('three');
  await expect.poll(() => pendingBinds(page)).toBe(0);
  for (let i = 0; i < 5; i += 1) await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('Enter');
  await expect(editor.locator('.ce-block')).toHaveCount(4);
  await expect(editor.locator('.ce-paragraph')).toHaveText([
    'one',
    'two',
    '',
    'three',
  ]);
});

test('Enter at the start of a just-created paragraph adds an empty paragraph above', async ({
  page,
}) => {
  await trackBlockEventBinding(page);
  const { created } = await setup(page);
  const editor = await typeInNewTask(page);
  await page.keyboard.type('a');
  await page.keyboard.press('Enter');
  await page.keyboard.type('text');
  for (let i = 0; i < 4; i += 1) await page.keyboard.press('ArrowLeft');
  await pressUnboundEnter(page);
  await page.keyboard.type('X');
  await expect(editor.locator('.ce-paragraph')).toHaveText(['a', '', 'Xtext']);
  for (let i = 0; i < 4; i += 1) await page.keyboard.press('ArrowRight');
  await pressUnboundEnter(page);
  await page.keyboard.type('Y');
  await expect(editor.locator('.ce-paragraph')).toHaveText([
    'a',
    '',
    'Xtext',
    'Y',
  ]);
  // Empty paragraphs are not saved (Paragraph tool, preserveBlank false).
  expect(await createTask(page, created)).toEqual([
    ['paragraph', 'a'],
    ['paragraph', 'Xtext'],
    ['paragraph', 'Y'],
  ]);
});

test('Enter inside bold text of a just-created paragraph keeps the markup on both halves', async ({
  page,
}) => {
  await trackBlockEventBinding(page);
  const { created } = await setup(page);
  const editor = await typeInNewTask(page);
  await page.keyboard.type('a');
  await page.keyboard.press('Enter');
  await editor
    .locator('.ce-paragraph')
    .nth(1)
    .evaluate((node) => {
      node.innerHTML = 'one<b>BOLD</b>two';
      const range = document.createRange();
      range.setStart(node.querySelector('b').firstChild, 2);
      range.collapse(true);
      window.getSelection().removeAllRanges();
      window.getSelection().addRange(range);
    });
  await pressUnboundEnter(page);
  await page.keyboard.type('Z');
  const html = await paragraphHtml(editor);
  expect(html.slice(0, 2)).toEqual(['a', 'one<b>BO</b>']);
  expect(html[2]).toMatch(/^(<b>)?Z(<\/b><b>)?LD<\/b>two$/);
  const saved = await createTask(page, created);
  expect(saved.slice(0, 2)).toEqual([
    ['paragraph', 'a'],
    ['paragraph', 'one<b>BO</b>'],
  ]);
  expect(saved[2][1]).toMatch(/LD<\/b>two$/);
});

test('Enter in the toolbox opened over a just-created block picks the item, no extra block', async ({
  page,
}) => {
  await trackBlockEventBinding(page);
  await setup(page);
  const editor = await typeInNewTask(page);
  await page.keyboard.type('a');
  await page.keyboard.press('Enter');
  const blocks = editor.locator('.ce-block');
  await blocks.first().hover();
  await blocks.last().hover();
  await page.locator('.new-form-container .ce-toolbar__plus').click();
  await expect(page.locator('.ce-popover--opened')).toHaveCount(1);
  await pressUnboundEnter(page);
  await expect(page.locator('.ce-popover--opened')).toHaveCount(1);
  expect(await paragraphHtml(editor)).toEqual(['a', '']);
  const header = page.locator(
    '.ce-popover--opened .ce-popover-item--focused[data-item-name="header"]',
  );
  for (let i = 0; i < 20 && !(await header.count()); i += 1)
    await page.keyboard.press('ArrowDown');
  await expect(header).toHaveCount(1);
  await pressUnboundEnter(page);
  await expect(page.locator('.ce-popover--opened')).toHaveCount(0);
  await expect(blocks).toHaveCount(2);
  await page.keyboard.type('H');
  await pressUnboundEnter(page);
  await page.keyboard.type('next');
  await expect(blocks).toHaveCount(3);
  await expect(editor.locator('.ce-header')).toHaveText('H');
  await expect(editor.locator('.ce-paragraph')).toHaveText(['a', 'next']);
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

// TM-588: Done and Create must not write a block description nobody edited.
// Keys in MySQL JSON column order (data before type), unlike Editor.js save().
const paragraphDoc = () => ({
  time: 1731156298664,
  blocks: [{ id: 'p1', data: { text: 'Untouched text' }, type: 'paragraph' }],
  version: '2.30.6',
});

const doneButton = (page) =>
  page.getByRole('button', { name: 'Done', exact: true });

test('Done on an untouched block description sends no write', async ({
  page,
}) => {
  const { errors, writes } = await setup(page, {
    task: () => ({ description_json: paragraphDoc() }),
  });
  await openTask(page, 2);
  const editor = await openEditor(page);
  await expect(editor.locator('.ce-paragraph')).toHaveText('Untouched text');
  // Longer than the 400ms Editor.js onChange batch.
  await page.waitForTimeout(1500);
  await doneButton(page).click();
  await expect(editor).toHaveCount(0);
  // Longer than the 2000ms autosave debounce.
  await page.waitForTimeout(2500);
  expect(writes).toEqual([]);
  expect(pageErrors(errors)).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

test('Done after an edit still saves the block description', async ({
  page,
}) => {
  const { errors, writes } = await setup(page, {
    task: () => ({ description_json: paragraphDoc() }),
  });
  await openTask(page, 2);
  const editor = await openEditor(page);
  const paragraph = editor.locator('.ce-paragraph').first();
  await paragraph.click();
  await caretAtEnd(paragraph);
  await page.keyboard.type('!');
  await doneButton(page).click();
  await expect.poll(() => writes.length, { timeout: 8000 }).toBe(1);
  expect(writes[0].method).toBe('PUT');
  expect(
    writes[0].body.description_json.blocks.map((block) => block.data.text),
  ).toEqual(['Untouched text!']);
  expect(pageErrors(errors)).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

test('Create with an untouched block description sends no description_json', async ({
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
  await page
    .locator('.new-form-container .codex-editor .ce-paragraph')
    .first()
    .waitFor();
  await page.locator('.new-form-container button[title="Create"]').click();
  await expect.poll(() => created.length).toBe(1);
  expect(created[0].description_json ?? null).toBeNull();
});

test('Done on an untouched legacy list task sends no write', async ({
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
  await page.waitForTimeout(1500);
  await doneButton(page).click();
  await expect(editor).toHaveCount(0);
  await page.waitForTimeout(2500);
  expect(writes).toEqual([]);
  expect(pageErrors(errors)).toEqual([]);
  expect(editorErrors(errors)).toEqual([]);
});

// D-001: an HTML string model compares as no blocks, so clearing it must still write.
test('clearing a legacy HTML description and pressing Done saves the empty description', async ({
  page,
}) => {
  const { errors, writes } = await setup(page, {
    task: () => ({ description: null, description_json: '<p>Legacy HTML</p>' }),
  });
  await openTask(page, 2);
  const editor = await openEditor(page);
  const paragraph = editor.locator('.ce-paragraph').first();
  await expect(paragraph).toHaveText('Legacy HTML');
  await paragraph.click();
  await page.keyboard.press('ControlOrMeta+A');
  await page.keyboard.press('Backspace');
  await expect(paragraph).toHaveText('');
  await page.waitForTimeout(1000);
  await doneButton(page).click();
  await expect.poll(() => writes.length, { timeout: 8000 }).toBeGreaterThan(0);
  const saved = writes.at(-1).body.description_json;
  expect(JSON.stringify(saved ?? null)).not.toContain('Legacy');
  expect(
    (saved?.blocks ?? []).filter((block) => block.data?.text),
  ).toEqual([]);
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

test('empty and malformed list blocks load without errors and keep unreadable items as text', async ({
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
  await expect(preview.locator('li')).toHaveText(['3']);
  const editor = await openEditor(page);
  await expect(
    editor.locator('.ce-paragraph', { hasText: 'only' }),
  ).toBeVisible();
  await expect(editor.locator('.cdx-list__item-content')).toHaveText(['3']);
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
