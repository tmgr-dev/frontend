import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test.setTimeout(90_000);

const SCREENSHOTS = process.env.QA_SCREENSHOTS_DIR;
const record = (label, value) =>
  console.log(`RECORD ${label}: ${JSON.stringify(value)}`);

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

const isExternal = (url) =>
  /^https?:/.test(url) && new URL(url).hostname !== '127.0.0.1';

const setup = async (page, task = {}, { colorScheme = 'light' } = {}) => {
  const errors = [];
  const external = new Set();
  const dialogs = [];
  const api = [];
  page.on('pageerror', (error) => errors.push(`pageerror: ${error.message}`));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(`console: ${message.text()}`);
  });
  page.on('dialog', (dialog) => {
    dialogs.push(dialog.message());
    dialog.dismiss().catch(() => {});
  });
  page.on('request', (request) => {
    const url = request.url();
    if (isExternal(url)) {
      const { hostname, pathname } = new URL(url);
      external.add(`${hostname}${pathname}`);
    } else if (url.includes('/api/'))
      api.push(`${request.method()} ${new URL(url).pathname}`);
  });
  // Hermetic: external hosts are recorded above and answered empty here.
  await page.route(isExternal, (route) =>
    route.fulfill({ status: 200, body: '' }),
  );
  await page.addInitScript((scheme) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('preferred_editor', 'markdown');
    localStorage.setItem('colorScheme', scheme);
  }, colorScheme);
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
  return { errors, external, dialogs, api, puts, current: () => current };
};

const openTask = async (page) => {
  await page.locator('[data-task-id="1"]').first().click();
  const form = page.locator('.new-form-container');
  await expect(form.getByPlaceholder('Task name')).toHaveValue('Original task');
  return form;
};

const openEditor = async (page, task = {}, options) => {
  const ctx = await setup(page, task, options);
  const form = await openTask(page);
  const edit = task.description
    ? form.getByRole('button', { name: 'Edit description' })
    : form.getByRole('button', { name: 'No description — click to add' });
  await edit.click();
  await expect(form.locator('.cm-content')).toBeVisible();
  return { ...ctx, form, content: form.locator('.cm-content') };
};

const docText = (page) =>
  page
    .locator('.new-form-container .cm-content')
    .evaluate((el) => {
      // innerText would include the placeholder. @codemirror/view < 6.43 exposes `cmView`,
      // 6.43+ `cmTile`; fail loudly if both disappear.
      const view = el.cmView?.view ?? el.cmTile?.root?.view;
      if (!view) throw new Error('CodeMirror view not found, update docText');
      return view.state.doc.toString();
    });

const selectAll = async (page, content) => {
  await content.click();
  await page.keyboard.press('ControlOrMeta+a');
};

const lightnessOf = (color) => {
  const ctx = document
    .createElement('canvas')
    .getContext('2d', { willReadFrequently: true });
  ctx.fillStyle = color;
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
  return { l: (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255, a };
};

const pasteInto = (content, { text, html, png }) =>
  content.evaluate(
    (el, { text, html, png }) => {
      const data = new DataTransfer();
      if (text) data.setData('text/plain', text);
      if (html) data.setData('text/html', html);
      if (png) {
        const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
        data.items.add(new File([bytes], 'pasted.png', { type: 'image/png' }));
      }
      el.focus();
      // false = some handler called preventDefault (the paste was handled, not ignored).
      return el.dispatchEvent(
        new ClipboardEvent('paste', {
          clipboardData: data,
          bubbles: true,
          cancelable: true,
        }),
      );
    },
    { text, html, png },
  );

// 1x1 transparent PNG.
const PNG =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';

/*
 * Baseline recorded on develop with md-editor-v3 4.21.3 (TM-583). The md-editor-v3 internals this
 * spec relies on (v6 may rename them): `.md-editor` (editor root), `.md-editor-dark`,
 * `.md-editor-fullscreen`, `.md-editor-toolbar` (toolbar container), toolbar `title` texts, and
 * CodeMirror's `.cm-content` + its `cmView` / `cmTile` property (docText). The v4 pop-up texts ("Add Link",
 * "Add Image", "Add Image Link", "Upload Images", placeholders) are only used on the tolerant v4 path.
 */

const TOOLBAR = [
  'bold',
  'underline',
  'italic',
  'strikeThrough',
  'title',
  'quote',
  'unordered list',
  'ordered list',
  'task list',
  'inline code',
  'block-level code',
  'link',
  'image',
  'table',
  'fullscreen in page',
];

// Requests to hosts other than 127.0.0.1 seen on develop (answered empty by `setup`).
// analytics.dzik.email comes from index.html; the rest is md-editor-v3 4.x CDN defaults.
// Criterion 12 is "no NEW hosts" (D-007): the assertion compares hostnames, not versioned paths,
// and is "subset of", not equality — losing a CDN load is fine.
// md-editor-v3 6.5.6 (TM-583) requests the same unpkg host with newer paths
// (@highlightjs/cdn-assets@11.11.1, katex@0.16.33, mermaid@11.12.3, prettier@3.8.1) and no alicdn.
const EXTERNAL_BASELINE = [
  'analytics.dzik.email/js/script.js',
  'at.alicdn.com/t/c/font_2605852_cmafimm6hot.js',
  'unpkg.com/@highlightjs/cdn-assets@11.10.0/highlight.min.js',
  'unpkg.com/@highlightjs/cdn-assets@11.10.0/styles/atom-one-dark.min.css',
  'unpkg.com/cropperjs@1.6.2/dist/cropper.min.css',
  'unpkg.com/cropperjs@1.6.2/dist/cropper.min.js',
  'unpkg.com/katex@0.16.11/dist/katex.min.css',
  'unpkg.com/katex@0.16.11/dist/katex.min.js',
  'unpkg.com/mermaid@11.3.0/dist/mermaid.min.js',
  'unpkg.com/prettier@3.3.3/plugins/markdown.js',
  'unpkg.com/prettier@3.3.3/standalone.js',
];

// Pre-existing mock gap (agent-work endpoint), unrelated to the editor.
const KNOWN_ERRORS = /Failed to load agent work/;

let active;
test.afterEach(async () => {
  if (!active) return;
  const external = [...active.external].sort();
  record('external hosts', external);
  const hosts = new Set(EXTERNAL_BASELINE.map((url) => url.split('/')[0]));
  expect(external.filter((url) => !hosts.has(url.split('/')[0]))).toEqual([]);
  expect(active.errors.filter((line) => !KNOWN_ERRORS.test(line))).toEqual([]);
  expect(active.dialogs).toEqual([]);
  active = undefined;
});

const track = (ctx) => (active = ctx);

const root = (page) => page.locator('.new-form-container .md-editor').first();
const toolbarButton = (page, title) =>
  page.locator('.new-form-container').getByTitle(title, { exact: true });

// Visible to the user: laid out, not display/visibility-hidden, no transparent ancestor
// (md-editor-v3 4.x keeps closed dropdowns in the DOM with opacity 0).
const shown = (locator) =>
  locator.evaluateAll((els) =>
    els.some((el) => {
      if (!el.getClientRects().length) return false;
      for (let n = el; n; n = n.parentElement) {
        const cs = getComputedStyle(n);
        if (
          cs.display === 'none' ||
          cs.visibility === 'hidden' ||
          Number(cs.opacity) === 0
        )
          return false;
      }
      return true;
    }),
  );

const putDescription = (ctx, text) =>
  expect
    .poll(() => ctx.puts.some((body) => body.description === text), {
      timeout: 10_000,
    })
    .toBe(true);

const shot = (locator, name) =>
  SCREENSHOTS && locator.screenshot({ path: `${SCREENSHOTS}/${name}.png` });

test('criterion 4: typed text autosaves, survives close/reopen and a fresh tab', async ({
  page,
  context,
}) => {
  const ctx = track(await openEditor(page));
  await ctx.content.click();
  await page.keyboard.type('Persisted markdown line 42');
  await putDescription(ctx, 'Persisted markdown line 42');

  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit('closeTaskModal'),
  );
  await expect(page.locator('.new-form-container')).toHaveCount(0);
  const form = await openTask(page);
  const wrapper = form.locator('.description-editor-wrapper');
  await expect(wrapper).toContainText('Persisted markdown line 42');
  await form.getByRole('button', { name: 'Edit description' }).click();
  await expect.poll(() => docText(page)).toBe('Persisted markdown line 42');

  // A new tab has no sessionStorage draft: only what the server got can show up.
  const fresh = await context.newPage();
  const freshCtx = await setup(fresh, ctx.current());
  const freshForm = await openTask(fresh);
  await expect(freshForm.locator('.description-editor-wrapper')).toContainText(
    'Persisted markdown line 42',
  );
  expect(freshCtx.errors.filter((l) => !KNOWN_ERRORS.test(l))).toEqual([]);
});

test('criterion 5: toolbar shows the develop button set; bold and bullet list insert markdown', async ({
  page,
}) => {
  const ctx = track(await openEditor(page));
  const titles = await page
    .locator('.new-form-container .md-editor-toolbar [title]')
    .evaluateAll((els) =>
      els.filter((el) => el.getClientRects().length).map((el) => el.title),
    );
  record('toolbar', titles);
  expect(titles).toEqual(TOOLBAR);
  for (const hidden of [
    'subscript',
    'superscript',
    'mermaid',
    'formula',
    'revoke',
    'undo revoke',
    'save',
    'prettier',
    'fullscreen',
    'preview',
    'preview only',
    'html preview',
    'catalog',
    'source code',
  ])
    await expect(toolbarButton(page, hidden)).toHaveCount(0);

  await selectAll(page, ctx.content);
  await page.keyboard.type('word');
  await selectAll(page, ctx.content);
  await toolbarButton(page, 'bold').click();
  await expect.poll(() => docText(page)).toBe('**word**');
  await putDescription(ctx, '**word**');

  await selectAll(page, ctx.content);
  await page.keyboard.type('item');
  await selectAll(page, ctx.content);
  await toolbarButton(page, 'unordered list').click();
  await expect.poll(() => docText(page)).toBe('- item');
  await putDescription(ctx, '- item');
});

for (const scheme of ['light', 'dark'])
  test(`criterion 6: ${scheme} theme — dark class only in dark, no white box in dark`, async ({
    page,
  }) => {
    const ctx = track(
      await openEditor(page, {}, { colorScheme: scheme }),
    );
    await ctx.content.click();
    await page.keyboard.type('Theme check text');
    const editor = root(page);
    if (scheme === 'dark') await expect(editor).toHaveClass(/\bmd-editor-dark\b/);
    else await expect(editor).not.toHaveClass(/\bmd-editor-dark\b/);

    const wrapper = page.locator('.new-form-container .description-editor-wrapper');
    const look = await wrapper.evaluate((w, lightnessSrc) => {
      const lightnessOf = new Function(`return (${lightnessSrc})`)();
      const content = w.querySelector('.cm-content');
      let effective;
      for (let n = content; n && !effective; n = n.parentElement) {
        const bg = getComputedStyle(n).backgroundColor;
        if (lightnessOf(bg).a > 0) effective = bg;
      }
      const whiteBoxes = [w, ...w.querySelectorAll('*')]
        .filter((el) => el.getClientRects().length && el.offsetWidth > 24)
        .filter((el) => {
          for (let n = el; n; n = n.parentElement)
            if (Number(getComputedStyle(n).opacity) === 0) return false;
          const { l, a } = lightnessOf(getComputedStyle(el).backgroundColor);
          return a > 128 && l > 0.85;
        })
        .map((el) => `${el.tagName}.${String(el.className).slice(0, 50)}`);
      return {
        effective,
        effectiveLightness: lightnessOf(effective).l,
        sunken: getComputedStyle(w).backgroundColor,
        whiteBoxes,
      };
    }, lightnessOf.toString());
    record(`${scheme} theme`, look);
    await shot(wrapper, `md-editor-${scheme}`);
    if (scheme === 'dark') {
      expect(look.effectiveLightness).toBeLessThan(0.3);
      expect(look.whiteBoxes).toEqual([]);
    } else {
      // Light: the editor area shows the project --bg-sunken, same as the wrapper.
      expect(look.effective).toBe(look.sunken);
    }

    // md-editor-v3 6.x draws its own scrollbar (`.md-editor-custom-scrollbar__track/__thumb`,
    // absent in 4.x). The track stays transparent like 4.x's overlay scrollbar; the thumb shows
    // once the content overflows.
    const scrollbar = () =>
      wrapper.evaluate((w) => {
        const track = w.querySelector('.cm-editor .md-editor-custom-scrollbar__track, .md-editor-input-wrapper .md-editor-custom-scrollbar__track')
          ?? w.querySelector('.md-editor-custom-scrollbar__track');
        if (!track) return null;
        const thumb = track.parentElement.querySelector('.md-editor-custom-scrollbar__thumb');
        const box = thumb?.getBoundingClientRect();
        return {
          track: getComputedStyle(track).backgroundColor,
          thumb: thumb && getComputedStyle(thumb).backgroundColor,
          thumbHeight: box ? Math.round(box.height) : 0,
          thumbOpacity: thumb ? Number(getComputedStyle(thumb).opacity) : 0,
        };
      });
    const idle = await scrollbar();
    record(`${scheme} scrollbar idle`, idle);
    if (idle) {
      expect(idle.track).toBe('rgba(0, 0, 0, 0)');
      await page.keyboard.insertText(
        Array.from({ length: 60 }, (_, i) => `\nOverflow line ${i}`).join(''),
      );
      await expect.poll(async () => (await scrollbar()).thumbHeight).toBeGreaterThan(10);
      const overflow = await scrollbar();
      record(`${scheme} scrollbar overflow`, overflow);
      expect(overflow.track).toBe('rgba(0, 0, 0, 0)');
      expect(overflow.thumb).not.toBe('rgba(0, 0, 0, 0)');
      expect(overflow.thumbOpacity).toBeGreaterThan(0);
      await shot(wrapper, `md-editor-${scheme}-overflow`);
    }
  });

test('criterion 7: page fullscreen toggles on and back off', async ({ page }) => {
  const ctx = track(await openEditor(page));
  const editor = root(page);
  const viewport = page.viewportSize();
  const before = await editor.boundingBox();
  expect(before.width).toBeLessThan(viewport.width - 50);

  await toolbarButton(page, 'fullscreen in page').click();
  await expect(editor).toHaveClass(/\bmd-editor-fullscreen\b/);
  await expect
    .poll(async () => {
      const box = await editor.boundingBox();
      return [Math.round(box.x), Math.round(box.y), Math.round(box.width)];
    })
    .toEqual([0, 0, viewport.width]);
  const full = await editor.boundingBox();
  record('page fullscreen box', full);
  // develop (v4) gave a 420px strip here: NewForm's `height: 420px !important` beat the library's
  // fullscreen rule. Fixed in TM-583 (D-006): fullscreen now covers the viewport.
  expect(full.height).toBeGreaterThan(420);
  expect(Math.abs(full.height - viewport.height)).toBeLessThanOrEqual(2);
  await shot(page, 'md-editor-page-fullscreen');

  await toolbarButton(page, 'fullscreen in page').click();
  await expect(editor).not.toHaveClass(/\bmd-editor-fullscreen\b/);
  await expect.poll(async () => (await editor.boundingBox()).width).toBeCloseTo(before.width, 0);
  expect(Math.round(before.height)).toBe(420);
  await expect.poll(async () => Math.round((await editor.boundingBox()).height)).toBe(420);
  await expect(ctx.content).toBeVisible();
});

// Criterion 8, develop baseline: md-editor-v3 4.x inserts pasted text/plain verbatim
// (no auto-linking, no URL-to-markdown conversion), ignores text/html, and a pasted PNG
// without onUploadImg inserts nothing and calls no API.
for (const { name, clip, expected } of [
  {
    name: 'a plain URL',
    clip: { text: 'https://example.com/page' },
    expected: 'Start https://example.com/page',
  },
  {
    name: 'a markdown link',
    clip: { text: '[Example](https://example.com/md)' },
    expected: 'Start [Example](https://example.com/md)',
  },
  {
    name: 'a markdown image',
    clip: { text: '![pic](https://example.com/pic.png)' },
    expected: 'Start ![pic](https://example.com/pic.png)',
  },
  {
    name: 'an HTML link (text/plain wins)',
    clip: {
      text: 'Example',
      html: '<a href="https://example.com/h">Example</a>',
    },
    expected: 'Start Example',
  },
  { name: 'a PNG image', clip: { png: PNG }, expected: 'Start ' },
])
  test(`criterion 8: pasting ${name} matches develop`, async ({ page }) => {
    // develop: every case is handled (default prevented), the PNG one by md-editor's file branch.
    const ctx = track(await openEditor(page));
    await ctx.content.click();
    await page.keyboard.type('Start ');
    await expect.poll(() => docText(page)).toBe('Start ');
    const apiBefore = ctx.api.length;
    const notPrevented = await pasteInto(ctx.content, clip);
    record(`paste ${name} notPrevented`, notPrevented);
    expect(notPrevented).toBe(false);
    await expect.poll(() => docText(page)).toBe(expected);
    // Give async paste handlers (image upload) time to act before checking for side effects.
    await page.waitForTimeout(1000);
    expect(await docText(page)).toBe(expected);
    // No upload or other write besides the description autosave.
    expect(
      ctx.api
        .slice(apiBefore)
        .filter((r) => !r.startsWith('GET ') && r !== 'PUT /api/tasks/1'),
    ).toEqual([]);
  });

// Criterion 8, toolbar link/image buttons.
// develop (4.x): "link" opens an "Add Link" modal (Desc / Link / OK); "image" opens a dropdown
// (Add Image Link / Upload Images / Clip Upload) and "Add Image Link" opens an "Add Image" modal.
// v6 removed these pop-ups (plan D-004): the buttons insert markdown directly. Both paths are
// accepted here; the spec annotates which one ran. What must hold either way: the button ends
// with link/image markdown in the document, and no page error.
const fillModal = async (page, title) => {
  const heading = page.getByText(title, { exact: true });
  if (!(await shown(heading))) return false;
  await page.getByPlaceholder('Enter a description...').filter({ visible: true }).fill('Docs');
  await page.getByPlaceholder('Enter a link...').filter({ visible: true }).fill('https://example.com/docs');
  await page.getByText('OK', { exact: true }).filter({ visible: true }).click();
  return true;
};

test('criterion 8: link button ends with link markdown (pop-up on v4, direct on v6)', async ({
  page,
}) => {
  const ctx = track(await openEditor(page));
  await ctx.content.click();
  await page.keyboard.type('Start ');
  await toolbarButton(page, 'link').click();
  await page.waitForTimeout(500);
  await shot(page, 'md-editor-link-button');
  const popup = await fillModal(page, 'Add Link');
  test.info().annotations.push({ type: 'link button', description: popup ? 'modal (v4)' : 'direct insert (v6, D-004)' });
  record('link button', { popup, doc: await docText(page) });
  if (popup) await expect.poll(() => docText(page)).toBe('Start [Docs](https://example.com/docs)');
  else await expect.poll(() => docText(page)).toMatch(/^Start .*\[[^\]]*\]\([^)]*\)/);
});

test('criterion 8: image button ends with image markdown (pop-up on v4, direct on v6)', async ({
  page,
}) => {
  const ctx = track(await openEditor(page));
  await ctx.content.click();
  await page.keyboard.type('Start ');
  await toolbarButton(page, 'image').click();
  await page.waitForTimeout(500);
  const dropdownItem = page.getByText('Add Image Link', { exact: true });
  const dropdown = await shown(dropdownItem);
  if (dropdown) {
    await expect(page.getByText('Upload Images', { exact: true })).toBeVisible();
    await shot(page, 'md-editor-image-dropdown');
    await dropdownItem.click();
    await page.waitForTimeout(500);
  }
  await shot(page, 'md-editor-image-button');
  const popup = await fillModal(page, 'Add Image');
  test.info().annotations.push({
    type: 'image button',
    description: `${dropdown ? 'dropdown' : 'no dropdown'}, ${popup ? 'modal (v4)' : 'direct insert (v6, D-004)'}`,
  });
  record('image button', { dropdown, popup, doc: await docText(page) });
  // develop's image modal appends a newline after the image markdown.
  if (popup) await expect.poll(() => docText(page)).toBe('Start ![Docs](https://example.com/docs)\n');
  else await expect.poll(() => docText(page)).toMatch(/^Start .*!\[[^\]]*\]\([^)]*\)/);
});

const PREVIEW_MD = [
  '# Preview heading',
  '',
  '- first bullet',
  '- second bullet',
  '',
  '[a link](https://example.com/x)',
  '',
  'Some `inline code` here',
  '',
  '```js',
  'const a = 1;',
  '```',
].join('\n');

for (const scheme of ['light', 'dark'])
  test(`criterion 9: ${scheme} preview renders heading, bullets, link, code with develop spacing`, async ({
    page,
  }) => {
    const ctx = track(await setup(page, { description: PREVIEW_MD }, { colorScheme: scheme }));
    const form = await openTask(page);
    const wrapper = form.locator('.description-editor-wrapper');
    await expect(wrapper.locator('h1')).toHaveText('Preview heading');
    await expect(wrapper.locator('ul > li')).toHaveText(['first bullet', 'second bullet']);
    await expect(wrapper.locator('a[href="https://example.com/x"]')).toHaveText('a link');
    await expect(wrapper.locator('p code')).toHaveText('inline code');
    await expect(wrapper.locator('pre')).toContainText('const a = 1;');
    await expect(form.locator('.cm-editor')).toHaveCount(0);

    const geometry = await wrapper.evaluate((w) => {
      const wb = w.getBoundingClientRect();
      const at = (sel) => {
        const b = w.querySelector(sel).getBoundingClientRect();
        return { left: b.left - wb.left, top: b.top - wb.top, right: wb.right - b.right };
      };
      const li = w.querySelector('ul > li');
      return {
        padding: getComputedStyle(w.querySelector('.description-markdown')).padding,
        h1: at('h1'),
        li: at('ul > li'),
        listStyle: getComputedStyle(li).listStyleType,
        marker: getComputedStyle(li, '::marker').content,
      };
    });
    record(`${scheme} preview geometry`, geometry);
    await shot(wrapper, `md-preview-${scheme}`);
    // develop: .description-markdown has no padding; content sits inside DescriptionPreview's
    // px-4 py-3 pr-10 (+1px border): h1 at 17/29 (16px top margin), bullets indented to 54.6px.
    expect(geometry.padding).toBe('0px');
    expect(geometry.h1.left).toBeCloseTo(17, 0);
    expect(geometry.h1.top).toBeCloseTo(29, 0);
    expect(geometry.h1.right).toBeCloseTo(41, 0);
    expect(Math.abs(geometry.li.left - 54.6)).toBeLessThanOrEqual(1);
    expect(geometry.listStyle).toBe('disc');
    expect(geometry.marker).not.toBe('none');
  });

const XSS_MD = [
  'Safe marker',
  '',
  '<img src=x onerror="window.__xss=1">',
  '',
  '<script>window.__xss=2</script>',
  '',
  '[x](javascript:window.__xss=3)',
  '',
  '<a href="javascript:window.__xss=4">y</a>',
].join('\n');

const expectNoXssInDom = async (page) => {
  expect(
    await page.evaluate(() => ({
      xss: window.__xss,
      onerror: document.querySelectorAll('.new-form-container [onerror]').length,
      scripts: document.querySelectorAll('.new-form-container script').length,
      jsHrefs: [...document.querySelectorAll('[href]')].filter((el) =>
        /^\s*javascript:/i.test(el.getAttribute('href')),
      ).length,
    })),
  ).toEqual({ xss: undefined, onerror: 0, scripts: 0, jsHrefs: 0 });
};

// develop (4.x built-in xss filter): <img> keeps no onerror (src emptied), <script> is shown as
// escaped text, [x](javascript:) stays literal text, <a href="javascript:"> gets href="".
test('criterion 10: script-bearing markdown does not execute in preview or editor', async ({
  page,
}) => {
  const ctx = track(await setup(page, { description: XSS_MD }));
  const form = await openTask(page);
  const wrapper = form.locator('.description-editor-wrapper');
  await expect(wrapper).toContainText('Safe marker');
  await page.waitForFunction(() =>
    [...document.querySelectorAll('.new-form-container .description-editor-wrapper img')].every(
      (img) => img.complete,
    ),
  );
  await expectNoXssInDom(page);
  const y = wrapper.getByText('y', { exact: true });
  if (await y.count()) await y.first().click();
  const x = wrapper.getByText('x', { exact: true });
  if (await x.count()) await x.first().click();
  await page.waitForTimeout(300);
  await expectNoXssInDom(page);
  record('xss preview html', await wrapper.locator('.description-markdown').innerHTML());

  await form.getByRole('button', { name: 'Edit description' }).click();
  await expect.poll(() => docText(page)).toBe(XSS_MD);
  await page.waitForTimeout(500);
  await expectNoXssInDom(page);
});

// TM-593: only https embeds of the Editor.js embed hosts (BlockEditor.vue) keep their src,
// always with the forced sandbox; every other iframe loses its src.
const SANDBOX = 'allow-scripts allow-same-origin allow-popups';
const ALLOWED_IFRAMES = {
  youtube: ['https://www.youtube.com/embed/abc', 'https://www.youtube.com/embed/abc'],
  codepen: ['https://codepen.io/u/embed/p?height=300', 'https://codepen.io/u/embed/p?height=300'],
  figma: ['https://www.figma.com/embed?embed_host=share&url=x', 'https://www.figma.com/embed?embed_host=share&url=x'],
  entities: ['https://www.youtube.com/embed/ent?a=1&amp;b=2', 'https://www.youtube.com/embed/ent?a=1&b=2'],
  'author-sandbox': ['https://www.youtube.com/embed/sb" sandbox="" srcdoc="x', 'https://www.youtube.com/embed/sb'],
  'author-allow': ['https://www.youtube.com/embed/al" allow="camera; microphone', 'https://www.youtube.com/embed/al'],
  'bad-then-good': ['https://evil.example/a" src="https://www.youtube.com/embed/bg', 'https://www.youtube.com/embed/bg'],
  'good-then-bad': ['https://www.youtube.com/embed/gb" src="https://evil.example/b', 'https://www.youtube.com/embed/gb'],
};
const BLOCKED_IFRAMES = {
  http: 'http://www.youtube.com/embed/abc',
  'off-list': 'https://evil.example/page',
  lookalike: 'https://www.youtube.com.evil.example/embed/abc',
  relative: '/demo/list',
  'protocol-relative': '//evil.example/proto',
  data: 'data:image/svg+xml;base64,PHN2Zy8+',
  'app-origin': 'http://127.0.0.1:4179/demo/list',
  'encoded-scheme': 'https&#58;//evil.example/encoded',
  javascript: 'javascript:window.__xss=6',
  port: 'https://www.youtube.com:8443/embed/abc',
  userinfo: 'https://user:pw@www.youtube.com/embed/abc',
};
const IFRAME_MD = [
  'Iframe marker',
  ...Object.entries(ALLOWED_IFRAMES).map(
    ([title, [src]]) => `<iframe title="${title}" src="${src}"></iframe>`,
  ),
  ...Object.entries(BLOCKED_IFRAMES).map(
    ([title, src]) => `<iframe title="${title}" src="${src}" sandbox=""></iframe>`,
  ),
  '<iframe title="srcdoc" srcdoc="hello"></iframe>',
].join('\n\n');

const iframesIn = (page, selector) =>
  page.locator(selector).evaluateAll((els) =>
    Object.fromEntries(
      els.map((el) => [
        el.title,
        ['src', 'sandbox', 'srcdoc', 'allow'].map((name) => el.getAttribute(name)),
      ]),
    ),
  );

test('TM-593: preview keeps only allowlisted https iframes, sandboxed; editor renders none', async ({
  page,
}) => {
  const ctx = track(await setup(page, { description: IFRAME_MD }));
  const form = await openTask(page);
  const wrapper = form.locator('.description-editor-wrapper');
  await expect(wrapper).toContainText('Iframe marker');
  const iframes = await iframesIn(page, '.new-form-container .description-markdown iframe');
  record('iframes', iframes);
  expect(iframes).toEqual({
    ...Object.fromEntries(
      Object.entries(ALLOWED_IFRAMES).map(([title, [, href]]) => [title, [href, SANDBOX, null, null]]),
    ),
    ...Object.fromEntries(
      Object.keys(BLOCKED_IFRAMES).map((title) => [title, [null, null, null, null]]),
    ),
    srcdoc: [null, null, null, null],
  });

  // The browser asked only the allowlisted hosts for frames.
  await page.waitForTimeout(500);
  const frameHosts = page
    .frames()
    .slice(1)
    .map((frame) => frame.url())
    .filter((url) => url !== 'about:blank')
    .map((url) => new URL(url).hostname);
  record('frame hosts', frameHosts);
  expect([...new Set(frameHosts)].sort()).toEqual(['codepen.io', 'www.figma.com', 'www.youtube.com']);
  expect([...ctx.external].filter((url) => url.includes('evil.example'))).toEqual([]);
  expect(await page.evaluate(() => window.__xss)).toBeUndefined();
  // Allowed embeds are expected traffic here, not new CDN hosts (afterEach check).
  for (const url of [...ctx.external])
    if (/^(www\.youtube\.com|codepen\.io|www\.figma\.com)\//.test(url)) ctx.external.delete(url);

  await form.getByRole('button', { name: 'Edit description' }).click();
  await expect.poll(() => docText(page)).toBe(IFRAME_MD);
  await page.waitForTimeout(500);
  expect(await iframesIn(page, '.new-form-container iframe')).toEqual({});
  expect(await page.evaluate(() => window.__xss)).toBeUndefined();
});

test('criterion 11: empty description shows the empty preview and the editor placeholder', async ({
  page,
}) => {
  const ctx = track(await setup(page));
  const form = await openTask(page);
  const empty = form.getByRole('button', { name: 'No description — click to add' });
  await expect(empty).toBeVisible();
  await expect(form.locator('.cm-editor')).toHaveCount(0);
  await empty.click();
  await expect(form.locator('.cm-content')).toBeVisible();
  await expect(form.getByText('Add description...', { exact: true })).toBeVisible();
  await expect.poll(() => docText(page)).toBe('');
  expect(ctx.puts).toEqual([]);
});
