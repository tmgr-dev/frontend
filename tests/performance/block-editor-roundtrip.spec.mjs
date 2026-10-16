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

const setup = async (page, { task } = {}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(`pageerror: ${error.message}`));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(`console: ${message.text()}`);
  });
  await page.addInitScript(() => {
    localStorage.setItem('preferred_editor', 'block');
    sessionStorage.setItem('tmgr:tours:auto', 'true');
  });
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
  if (task)
    await page.route('**/api/tasks/2', async (route) => {
      if (route.request().method() !== 'GET') return route.fallback();
      await route.fulfill({
        json: { data: { ...baseTask, id: 2, ...task() } },
      });
    });
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  return { errors, created };
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

const openToolbox = async (page, item) => {
  await page.keyboard.type('/');
  const option = page
    .locator(`.ce-popover--opened [data-item-name="${item}"]`)
    .first();
  await option.click();
};

const addBlock = async (page, item) => {
  const blocks = page.locator('.new-form-container .codex-editor .ce-block');
  // Typing hides the toolbar; it reappears when the hovered block changes.
  await blocks.first().hover();
  await blocks.last().hover();
  await page.locator('.new-form-container .ce-toolbar__plus').click();
  await page
    .locator(`.ce-popover--opened [data-item-name="${item}"]`)
    .first()
    .click();
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

  // Editor.js batches onChange for 400ms and Create does not flush the editor.
  await page.waitForTimeout(1000);
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
  expect(editorErrors(errors)).toEqual([]);
});

test('Create right after the last keystroke keeps that text in the request', async ({
  page,
}) => {
  test.skip(
    !process.env.RUN_KNOWN_BUGS,
    'Known bug: createTask in src/pages/NewForm.vue does not flush BlockEditor, so edits inside the 400ms Editor.js onChange batch are not sent',
  );
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
