import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test.setTimeout(60_000);

const ORIGINAL = 'Original description text';

const baseTask = {
  id: 1,
  title: 'Original task',
  description: null,
  description_json: null,
  status: 'created',
  status_id: 1,
  workspace_id: 1,
  user_id: 1,
  user: { id: 1, name: 'Test User' },
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

// Alphanumeric and without repeats, so a lost, doubled or reordered character shows.
const sequence = (length) =>
  Array.from({ length: Math.ceil(length / 2) }, (_, i) =>
    i.toString(36).padStart(2, '0'),
  )
    .join('')
    .slice(0, length);

// Thrown by unrelated task-panel code when the task-1 route is overridden (Chrome, WebKit).
const KNOWN_PAGE_ERRORS = [
  "Cannot read properties of undefined (reading 'filter')",
  "TypeError: undefined is not an object (evaluating 'result.nodes.filter')",
];
const isUnexpectedError = (message) => !KNOWN_PAGE_ERRORS.includes(message);

const setup = async (page, { editor, task } = {}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  if (test.info().project.name === 'chrome')
    await page
      .context()
      .grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.addInitScript((preferred) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('preferred_editor', preferred);
  }, editor);
  await mockApp(page);
  const created = [];
  await page.route('**/api/tasks', async (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    const body = route.request().postDataJSON();
    created.push(body);
    await route.fulfill({ json: { data: { ...baseTask, ...body, id: 2 } } });
  });
  if (task) {
    let current = { ...baseTask, ...task };
    await page.route('**/api/tasks/1', (route) => {
      if (route.request().method() === 'PUT')
        current = { ...current, ...route.request().postDataJSON() };
      return route.fulfill({ json: { data: current } });
    });
  }
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().waitFor();
  return { errors, created };
};

const openNewTask = async (page) => {
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  const form = page.locator('.new-form-container');
  await expect(form.getByPlaceholder('Task name')).toBeVisible();
  // NewForm re-focuses the title 350 ms after mounting a new task.
  await page.waitForTimeout(450);
  return form;
};

const openExistingTask = async (page) => {
  await page.locator('[data-task-id="1"]').first().click();
  const form = page.locator('.new-form-container');
  await expect(form.getByPlaceholder('Task name')).toHaveValue('Original task');
  return form;
};

const SURFACES = {
  markdown: { selector: '.cm-content' },
  blockmd: { selector: '.milkdown .ProseMirror' },
  block: { selector: '.codex-editor' },
};

// Text of the description as the editor shows it, one entry per paragraph/line.
const paragraphs = (form, editor) => {
  const root = form.locator(SURFACES[editor].selector);
  if (editor === 'markdown')
    return root.locator('.cm-line').evaluateAll((lines) =>
      lines.map((line) => {
        const copy = line.cloneNode(true);
        copy.querySelectorAll('.cm-placeholder').forEach((el) => el.remove());
        return copy.textContent;
      }),
    );
  if (editor === 'blockmd')
    return root.evaluate((node) =>
      Array.from(node.children).map((child) => child.textContent),
    );
  return root
    .locator('.ce-paragraph')
    .evaluateAll((nodes) => nodes.map((node) => node.textContent));
};

// markdown keeps the blank line between paragraphs as an empty line
const nonEmpty = async (form, editor) =>
  (await paragraphs(form, editor)).filter((text) => text !== '');

const expectDescriptionFocused = (page) =>
  expect
    .poll(() =>
      page.evaluate(
        () => !!document.activeElement?.closest('.description-editor-wrapper'),
      ),
    )
    .toBe(true);

const writeClipboard = (page, text) =>
  page.evaluate((value) => navigator.clipboard.writeText(value), text);

const fillTitle = async (form, text) => {
  const title = form.getByPlaceholder('Task name');
  await title.fill(text);
  await title.press('End');
  return title;
};

for (const editor of ['markdown', 'blockmd', 'block']) {
  test(`${editor}: typing past 255 characters continues in the description without losing any`, async ({
    page,
  }) => {
    const { created, errors } = await setup(page, { editor });
    const form = await openNewTask(page);
    const typed = sequence(300);
    await form.getByPlaceholder('Task name').click();
    await page.keyboard.type(typed);

    const title = form.getByPlaceholder('Task name');
    await expect(title).toHaveValue(typed.slice(0, 255));
    await expect(form.getByText('255/255 characters')).toBeVisible();
    await expect.poll(() => nonEmpty(form, editor)).toEqual([typed.slice(255)]);
    await expectDescriptionFocused(page);

    await page.keyboard.type('Z');
    await expect
      .poll(() => nonEmpty(form, editor))
      .toEqual([`${typed.slice(255)}Z`]);
    await expect(title).toHaveValue(typed.slice(0, 255));

    if (editor === 'block') await page.waitForTimeout(800);
    await form.locator('button[title="Create"]').click();
    await expect.poll(() => created.length).toBe(1);
    expect(created[0].title).toBe(typed.slice(0, 255));
    const saved =
      editor === 'block'
        ? created[0].description_json.blocks.map((block) => block.data.text)
        : [created[0].description];
    expect(saved.join('\n')).toContain(`${typed.slice(255)}Z`);
    expect(errors).toEqual([]);
  });
}

for (const editor of ['markdown', 'blockmd'])
  test(`${editor}: a paste that does not fit is cut at a word, the rest goes before the existing description`, async ({
    page,
  }) => {
    await setup(page, { editor });
    const form = await openNewTask(page);
    const surface = form.locator(SURFACES[editor].selector);
    await surface.click();
    await page.keyboard.type('Existing text');
    await expect.poll(() => nonEmpty(form, editor)).toEqual(['Existing text']);

    const title = await fillTitle(form, 'x'.repeat(250));
    await writeClipboard(page, 'ab cd efgh ijkl');
    await page.keyboard.press('ControlOrMeta+v');

    await expect(title).toHaveValue(`${'x'.repeat(250)}ab cd`);
    await expect
      .poll(() => nonEmpty(form, editor))
      .toEqual(['efgh ijkl', 'Existing text']);
    await expectDescriptionFocused(page);
    await page.keyboard.type('Q');
    await expect
      .poll(() => nonEmpty(form, editor))
      .toEqual(['efgh ijklQ', 'Existing text']);
  });

test('markdown: a paste in the middle of the title keeps the text after the caret and Cmd+Z removes the carried paragraph', async ({
  page,
}) => {
  await setup(page, { editor: 'markdown' });
  const form = await openNewTask(page);
  const title = await fillTitle(form, `${'a'.repeat(100)}${'b'.repeat(150)}`);
  for (let i = 0; i < 150; i += 1) await title.press('ArrowLeft');
  await writeClipboard(page, 'hello world, more');
  await page.keyboard.press('ControlOrMeta+v');

  await expect(title).toHaveValue(`${'a'.repeat(100)}hello${'b'.repeat(150)}`);
  await expect.poll(() => nonEmpty(form, 'markdown')).toEqual(['world, more']);
  await page.waitForTimeout(600);
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(() => nonEmpty(form, 'markdown')).toEqual([]);
});

test('the title part of an overflowing paste can be undone in the title', async ({
  page,
}) => {
  await setup(page, { editor: 'markdown' });
  const form = await openNewTask(page);
  const title = form.getByPlaceholder('Task name');
  await title.click();
  await page.keyboard.type('x'.repeat(250));
  await writeClipboard(page, 'ab cd efgh ijkl');
  await page.keyboard.press('ControlOrMeta+v');
  await expect(title).toHaveValue(`${'x'.repeat(250)}ab cd`);
  await expect.poll(() => nonEmpty(form, 'markdown')).toEqual(['efgh ijkl']);
  await title.click();
  await page.keyboard.press('ControlOrMeta+z');
  // WebKit coalesces the typed text and the insert into one undo step.
  await expect(title).not.toHaveValue(`${'x'.repeat(250)}ab cd`);
  expect((await title.inputValue()).length).toBeLessThanOrEqual(250);
});

test('blockmd: Cmd+Z in the description removes the carried paragraph', async ({
  page,
}) => {
  await setup(page, { editor: 'blockmd' });
  const form = await openNewTask(page);
  await fillTitle(form, 'x'.repeat(255));
  await writeClipboard(page, 'carried over text');
  await page.keyboard.press('ControlOrMeta+v');
  await expect
    .poll(() => nonEmpty(form, 'blockmd'))
    .toEqual(['carried over text']);
  await page.waitForTimeout(600);
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(() => nonEmpty(form, 'blockmd')).toEqual([]);
});

test('a paste replacing a selection counts the selection as removed', async ({
  page,
}) => {
  await setup(page, { editor: 'markdown' });
  const form = await openNewTask(page);
  const title = await fillTitle(form, `${'a'.repeat(20)}${'b'.repeat(235)}`);
  await title.evaluate((el) => el.setSelectionRange(0, 10));
  await writeClipboard(page, 'x'.repeat(10));
  await page.keyboard.press('ControlOrMeta+v');
  await expect(title).toHaveValue(
    `${'x'.repeat(10)}${'a'.repeat(10)}${'b'.repeat(235)}`,
  );
  await expect.poll(() => nonEmpty(form, 'markdown')).toEqual([]);

  await title.evaluate((el) => el.setSelectionRange(0, 10));
  await writeClipboard(page, 'y'.repeat(15));
  await page.keyboard.press('ControlOrMeta+v');
  await expect(title).toHaveValue(
    `${'y'.repeat(10)}${'a'.repeat(10)}${'b'.repeat(235)}`,
  );
  await expect.poll(() => nonEmpty(form, 'markdown')).toEqual(['yyyyy']);
});

test('a paste that fits behaves as before and keeps focus in the title', async ({
  page,
}) => {
  await setup(page, { editor: 'markdown' });
  const form = await openNewTask(page);
  const title = await fillTitle(form, 'abc');
  await writeClipboard(page, 'def ghi');
  await page.keyboard.press('ControlOrMeta+v');
  await expect(title).toHaveValue('abcdef ghi');
  await expect(title).toBeFocused();
  await expect.poll(() => nonEmpty(form, 'markdown')).toEqual([]);
  await page.keyboard.press('ControlOrMeta+z');
  await expect(title).not.toHaveValue('abcdef ghi');
});

test('typing in the middle of a full title stays blocked and moves nothing', async ({
  page,
}) => {
  await setup(page, { editor: 'markdown' });
  const form = await openNewTask(page);
  const title = await fillTitle(form, 'x'.repeat(255));
  for (let i = 0; i < 3; i += 1) await title.press('ArrowLeft');
  await page.keyboard.type('Q');
  await expect(title).toHaveValue('x'.repeat(255));
  await expect(title).toBeFocused();
  await expect.poll(() => nonEmpty(form, 'markdown')).toEqual([]);
});

test('IME composition and insertText input are handled without breaking', async ({
  page,
}) => {
  await setup(page, { editor: 'markdown' });
  const form = await openNewTask(page);
  const title = await fillTitle(form, 'x'.repeat(255));
  const prevented = await title.evaluate((el) => {
    const event = new KeyboardEvent('keydown', {
      key: 'a',
      isComposing: true,
      bubbles: true,
      cancelable: true,
    });
    el.dispatchEvent(event);
    return event.defaultPrevented;
  });
  expect(prevented).toBe(false);
  await expect.poll(() => nonEmpty(form, 'markdown')).toEqual([]);

  // WebKit gives beforeinput an empty `data` at maxlength: only keydown carries there.
  test.skip(test.info().project.name === 'webkit', 'no beforeinput data');
  await page.keyboard.insertText('IME');
  await expect(title).toHaveValue('x'.repeat(255));
  await expect.poll(() => nonEmpty(form, 'markdown')).toEqual(['IME']);
});

for (const editor of ['markdown', 'blockmd'])
  test(`${editor}: overflow on an existing task opens the editor from the preview, keeps every typed character and Cancel reverts`, async ({
    page,
  }) => {
    await setup(page, { editor, task: { description: ORIGINAL } });
    const form = await openExistingTask(page);
    await expect(form.locator('.description-editor-wrapper')).toContainText(
      ORIGINAL,
    );
    await expect(form.locator(SURFACES[editor].selector)).toHaveCount(0);

    const title = await fillTitle(form, 'x'.repeat(255));
    const typed = sequence(40);
    await page.keyboard.type(typed);
    await expect(title).toHaveValue('x'.repeat(255));
    await expect.poll(() => nonEmpty(form, editor)).toEqual([typed, ORIGINAL]);
    await expectDescriptionFocused(page);

    await form.getByRole('button', { name: 'Cancel', exact: true }).click();
    const wrapper = form.locator('.description-editor-wrapper');
    await expect(wrapper).toContainText(ORIGINAL);
    await expect(wrapper).not.toContainText(typed);
  });

test('block: overflow on an existing Editor.js task is inserted before the existing blocks', async ({
  page,
}) => {
  const { errors } = await setup(page, {
    editor: 'block',
    task: {
      description_json: {
        time: 1,
        version: '2.30.6',
        blocks: [
          { id: 'p1', type: 'paragraph', data: { text: 'Existing block' } },
        ],
      },
    },
  });
  const form = await openExistingTask(page);
  await expect(form.locator('.description-editor-wrapper')).toContainText(
    'Existing block',
  );
  await fillTitle(form, 'x'.repeat(255));
  await page.keyboard.type('a < b & c');
  await expect
    .poll(() => nonEmpty(form, 'block'))
    .toEqual(['a < b & c', 'Existing block']);
  await expectDescriptionFocused(page);
  await page.keyboard.type('!');
  await expect
    .poll(() => nonEmpty(form, 'block'))
    .toEqual(['a < b & c!', 'Existing block']);
  expect(errors.filter(isUnexpectedError)).toEqual([]);
});

test('block: overflow on an existing legacy HTML task is not wiped by the initial render', async ({
  page,
}) => {
  const { errors } = await setup(page, {
    editor: 'markdown',
    task: { description_json: '<p>Existing</p>' },
  });
  const form = await openExistingTask(page);
  await expect(form.locator('.description-editor-wrapper')).toContainText(
    'Existing',
  );
  await fillTitle(form, 'x'.repeat(255));
  const typed = sequence(10);
  await page.keyboard.type(typed);
  await expect.poll(() => nonEmpty(form, 'block')).toEqual([typed, 'Existing']);
  await expectDescriptionFocused(page);
  await page.keyboard.type('!');
  await expect
    .poll(() => nonEmpty(form, 'block'))
    .toEqual([`${typed}!`, 'Existing']);
  expect(errors.filter(isUnexpectedError)).toEqual([]);
});
