import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

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

const blocks = (...list) => ({ time: 1, version: '2.30.6', blocks: list });

const openTask = async (
  page,
  task = {},
  { colorScheme = 'light', editor = 'markdown' } = {},
) => {
  await page.addInitScript(
    ([scheme, preferred]) => {
      sessionStorage.setItem('tmgr:tours:auto', 'true');
      localStorage.setItem('preferred_editor', preferred);
      localStorage.setItem('colorScheme', scheme);
    },
    [colorScheme, editor],
  );
  await mockApp(page);
  let current = { ...baseTask, ...task };
  const puts = [];
  await page.route('**/api/tasks/1', (route) => {
    if (route.request().method() === 'PUT') {
      const body = route.request().postDataJSON();
      puts.push(body);
      current = { ...current, ...body };
    }
    return route.fulfill({ json: { data: current } });
  });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const form = page.locator('.new-form-container');
  await expect(form.getByPlaceholder('Task name')).toHaveValue('Original task');
  return { form, puts };
};

const replaceMarkdown = async (page, form, text) => {
  await form.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.type(text);
};

const editAndWaitForSave = async (page, form, puts) => {
  await form.getByRole('button', { name: 'Edit description' }).click();
  await replaceMarkdown(page, form, 'Changed text');
  await expect
    .poll(() => puts.some((body) => body.description === 'Changed text'), {
      timeout: 8000,
    })
    .toBe(true);
};

const expectPreviewMode = async (form) => {
  await expect(form.locator('.cm-editor')).toHaveCount(0);
  await expect(form.locator('.md-editor-toolbar')).toHaveCount(0);
  await expect(form.locator('.codex-editor')).toHaveCount(0);
  await expect(
    form.getByRole('button', { name: 'Edit description' }),
  ).toBeVisible();
};

const lightness = (locator) =>
  locator.evaluate((el) => {
    const ctx = document
      .createElement('canvas')
      .getContext('2d', { willReadFrequently: true });
    ctx.fillStyle = getComputedStyle(el).color;
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
    return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  });

const expectNoXss = async (page, preview) => {
  await expect(preview.locator('[onerror]')).toHaveCount(0);
  await expect(preview.locator('script')).toHaveCount(0);
  await page.waitForFunction(() =>
    Array.from(
      document.querySelectorAll(
        '.new-form-container .description-editor-wrapper img',
      ),
    ).every((img) => img.complete),
  );
  expect(await page.evaluate(() => window.__xss)).toBeUndefined();
};

test('an existing markdown description opens as a rendered preview with an Edit button', async ({
  page,
}) => {
  const { form } = await openTask(page, {
    description: `# Heading here\n\n${ORIGINAL} with **bold**`,
  });
  const wrapper = form.locator('.description-editor-wrapper');
  await expect(wrapper.locator('h1')).toHaveText('Heading here');
  await expect(wrapper.locator('strong')).toHaveText('bold');
  await expect(wrapper).toContainText(ORIGINAL);
  await expectPreviewMode(form);
  await expect(
    form.getByRole('button', { name: 'Done', exact: true }),
  ).toHaveCount(0);
});

for (const editor of ['markdown', 'blockmd'])
  test(`${editor} lists in the preview keep their bullets and numbers`, async ({
    page,
  }) => {
    const { form } = await openTask(
      page,
      { description: '- first\n- second\n\n1. one\n2. two' },
      { editor },
    );
    const wrapper = form.locator('.description-editor-wrapper');
    const marker = (selector) =>
      wrapper
        .locator(selector)
        .first()
        .evaluate((el) => getComputedStyle(el).listStyleType);
    await expect.poll(() => marker('ul > li')).toBe('disc');
    await expect.poll(() => marker('ol > li')).toBe('decimal');
  });

test('Edit then Done shows the new text in the preview and saves it', async ({
  page,
}) => {
  const { form, puts } = await openTask(page, { description: ORIGINAL });
  await form.getByRole('button', { name: 'Edit description' }).click();
  await expect(form.locator('.cm-editor')).toBeVisible();
  await replaceMarkdown(page, form, 'Fresh text from Done');
  await form.getByRole('button', { name: 'Done', exact: true }).click();

  const wrapper = form.locator('.description-editor-wrapper');
  await expect(wrapper).toContainText('Fresh text from Done');
  await expect(wrapper).not.toContainText(ORIGINAL);
  await expectPreviewMode(form);
  await expect
    .poll(() => puts.at(-1)?.description, { timeout: 8000 })
    .toBe('Fresh text from Done');
});

test('Edit then Cancel restores the old text in the preview and on the server', async ({
  page,
}) => {
  const { form, puts } = await openTask(page, { description: ORIGINAL });
  await editAndWaitForSave(page, form, puts);
  await form.getByRole('button', { name: 'Cancel', exact: true }).click();

  const wrapper = form.locator('.description-editor-wrapper');
  await expect(wrapper).toContainText(ORIGINAL);
  await expect(wrapper).not.toContainText('Changed text');
  await expectPreviewMode(form);
  await expect
    .poll(() => puts.at(-1)?.description, { timeout: 8000 })
    .toBe(ORIGINAL);
});

test('Escape while editing reverts like Cancel and keeps the task modal open', async ({
  page,
}) => {
  const { form, puts } = await openTask(page, { description: ORIGINAL });
  await editAndWaitForSave(page, form, puts);
  await page.keyboard.press('Escape');

  await expect(form).toBeVisible();
  await expect(form.getByPlaceholder('Task name')).toHaveValue('Original task');
  const wrapper = form.locator('.description-editor-wrapper');
  await expect(wrapper).toContainText(ORIGINAL);
  await expect(wrapper).not.toContainText('Changed text');
  await expectPreviewMode(form);
  await expect
    .poll(() => puts.at(-1)?.description, { timeout: 8000 })
    .toBe(ORIGINAL);
});

test('an empty description shows a placeholder that opens the editor', async ({
  page,
}) => {
  const { form } = await openTask(page);
  const placeholder = form.getByRole('button', {
    name: 'No description — click to add',
  });
  await expect(placeholder).toBeVisible();
  await expect(form.locator('.cm-editor')).toHaveCount(0);
  await placeholder.click();
  await expect(form.locator('.cm-editor')).toBeVisible();
  await expect(
    form.getByRole('button', { name: 'Done', exact: true }),
  ).toBeVisible();
  await expect(
    form.getByRole('button', { name: 'Cancel', exact: true }),
  ).toBeVisible();
});

test('an Editor.js description with only empty lists shows the placeholder', async ({
  page,
}) => {
  const { form } = await openTask(
    page,
    {
      description_json: blocks(
        { type: 'list', data: { style: 'unordered', items: ['', '<br>'] } },
        { type: 'checklist', data: { items: [{ text: '', checked: true }] } },
      ),
    },
    { editor: 'block' },
  );
  await expect(
    form.getByRole('button', { name: 'No description — click to add' }),
  ).toBeVisible();
});

test('the new task form opens the editor immediately, without a preview', async ({
  page,
}) => {
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('preferred_editor', 'markdown');
  });
  await mockApp(page);
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().waitFor();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  const form = page.locator('.new-form-container');
  await expect(form.getByTitle('Create')).toBeVisible();
  await expect(form.locator('.cm-editor')).toBeVisible();
  await expect(
    form.getByRole('button', { name: 'Edit description' }),
  ).toHaveCount(0);
  await expect(form.getByText('No description — click to add')).toHaveCount(0);
  await expect(
    form.getByRole('button', { name: 'Done', exact: true }),
  ).toHaveCount(0);
});

test('an Editor.js description renders every block type read-only', async ({
  page,
}) => {
  const { form } = await openTask(page, {
    description_json: blocks(
      { type: 'header', data: { text: 'Plan header', level: 2 } },
      {
        type: 'paragraph',
        data: {
          text: 'Some <b>bold</b>, <code class="inline-code">inline()</code> and <mark class="cdx-marker">marked</mark>',
        },
      },
      {
        type: 'list',
        data: { style: 'unordered', items: ['First item', 'Second item'] },
      },
      {
        type: 'list',
        data: {
          style: 'ordered',
          items: [
            {
              content: 'Step one',
              items: [{ content: 'Sub step', items: [] }],
            },
          ],
        },
      },
      {
        type: 'checklist',
        data: {
          items: [
            { text: 'Done thing', checked: true },
            { text: 'Open thing', checked: false },
          ],
        },
      },
      { type: 'quote', data: { text: 'Quoted text', caption: 'Someone' } },
      { type: 'warning', data: { title: 'Careful', message: 'Hot stove' } },
      { type: 'delimiter', data: {} },
      { type: 'code', data: { code: '<b>x</b> = 1' } },
      {
        type: 'table',
        data: {
          withHeadings: true,
          content: [
            ['Col A', 'Col B'],
            ['a1', 'b1'],
          ],
        },
      },
      {
        type: 'embed',
        data: {
          service: 'youtube',
          source: 'https://youtube.com/watch?v=1',
          embed: 'https://www.youtube.com/embed/1',
        },
      },
      { type: 'mystery', data: { text: 'Unknown block' } },
    ),
  });
  const preview = form.locator('.description-blocks');
  await expect(preview.locator('h2')).toHaveText('Plan header');
  await expect(preview.locator('p b')).toHaveText('bold');
  await expect(preview.locator('p code')).toHaveText('inline()');
  await expect(preview.locator('p mark')).toHaveText('marked');
  await expect(preview.locator('ul:not(.checklist) > li')).toHaveText([
    'First item',
    'Second item',
  ]);
  await expect(preview.locator('ol > li').first()).toContainText('Step one');
  await expect(preview.locator('ol > li ol > li')).toHaveText('Sub step');
  const checks = preview.locator('ul.checklist li');
  await expect(checks).toHaveText(['Done thing', 'Open thing']);
  await expect(checks.nth(0).locator('input[type="checkbox"]')).toBeChecked();
  await expect(
    checks.nth(1).locator('input[type="checkbox"]'),
  ).not.toBeChecked();
  await expect(preview.locator('blockquote')).toContainText('Quoted text');
  await expect(preview.locator('blockquote figcaption')).toHaveText('Someone');
  await expect(preview.locator('.warning')).toContainText('Careful');
  await expect(preview.locator('.warning')).toContainText('Hot stove');
  await expect(preview.locator('hr')).toHaveCount(1);
  await expect(preview.locator('pre code')).toHaveText('<b>x</b> = 1');
  await expect(preview.locator('pre b')).toHaveCount(0);
  await expect(preview.locator('th')).toHaveText(['Col A', 'Col B']);
  await expect(preview.locator('td')).toHaveText(['a1', 'b1']);
  await expect(
    preview.locator('a[href="https://youtube.com/watch?v=1"]'),
  ).toHaveText('youtube');
  await expect(preview.locator('iframe')).toHaveCount(0);
  await expect(preview).not.toContainText('Unknown block');
  await expectPreviewMode(form);
});

test('a code block saved by the block editor shows its code, not markup', async ({
  page,
}) => {
  const { form } = await openTask(page, {
    description_json: blocks({
      type: 'codebox',
      data: {
        code: '<span class="hljs-keyword">const</span> a = <span class="hljs-number">1</span>;<br>a &lt; 2',
        language: 'javascript',
      },
    }),
  });
  const code = form.locator('.description-blocks pre');
  await expect(code).toContainText('const a = 1;');
  await expect(code).toContainText('a < 2');
  await expect(code).not.toContainText('<span');
  await expect(code).not.toContainText('&lt;');
});

test('HTML in an Editor.js description does not execute', async ({ page }) => {
  const { form } = await openTask(page, {
    description_json: blocks(
      { type: 'paragraph', data: { text: 'Safe marker' } },
      {
        type: 'paragraph',
        data: { text: '<img src=x onerror="window.__xss=1">' },
      },
      {
        type: 'raw',
        data: {
          html: '<img src=y onerror="window.__xss=2"><script>window.__xss=3</script>',
        },
      },
      {
        type: 'header',
        data: { text: '<svg onload="window.__xss=4"></svg>Head', level: 2 },
      },
    ),
  });
  const preview = form.locator('.description-blocks');
  await expect(preview).toContainText('Safe marker');
  await expect(preview.locator('h2')).toHaveText('Head');
  await expectNoXss(page, preview);
});

test('HTML in a markdown description does not execute', async ({ page }) => {
  const { form } = await openTask(page, {
    description: [
      'Safe marker',
      '',
      '<img src=x onerror="window.__xss=1">',
      '',
      '<script>window.__xss=3</script>',
      '',
      '<p onmouseover="window.__xss=5">Hover me</p>',
    ].join('\n'),
  });
  const preview = form.locator('.description-editor-wrapper');
  await expect(preview).toContainText('Safe marker');
  await expectNoXss(page, preview);
  await expect(preview.locator('[onmouseover]')).toHaveCount(0);
});

for (const { kind, task, text } of [
  { kind: 'markdown', task: { description: ORIGINAL }, text: ORIGINAL },
  {
    kind: 'block',
    task: {
      description_json: blocks({
        type: 'paragraph',
        data: { text: 'Block body' },
      }),
    },
    text: 'Block body',
  },
]) {
  test(`the ${kind} preview text follows the light and dark theme`, async ({
    page,
  }) => {
    const { form } = await openTask(page, task);
    const body = form
      .locator('.description-editor-wrapper')
      .getByText(text, { exact: true });
    await expect(body).toBeVisible();
    expect(await lightness(body)).toBeLessThan(0.4);

    await page.evaluate(() =>
      document
        .querySelector('#app')
        .__vue_app__.config.globalProperties.$store.commit(
          'setColorScheme',
          'dark',
        ),
    );
    await expect(page.locator('html')).toHaveClass(/\bdark\b/);
    await expect.poll(() => lightness(body)).toBeGreaterThan(0.5);
  });
}
