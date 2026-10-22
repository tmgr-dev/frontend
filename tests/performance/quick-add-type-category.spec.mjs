import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const PATTERN_KEY = 'task_name_pattern_date&time';
const CATEGORIES = [
  {
    id: 10,
    title: 'Work',
    project_category_id: null,
    deleted_at: null,
    settings: [{ id: 1, key: PATTERN_KEY, value: 'TM-{index#category}' }],
  },
  {
    id: 11,
    title: 'Home',
    project_category_id: null,
    deleted_at: null,
    settings: [],
  },
];
const PNG =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

const setup = async (
  page,
  {
    failCategories = false,
    failIndexes = false,
    remembered = null,
    holdCategories = false,
  } = {},
) => {
  let releaseCategories = () => {};
  const categoriesGate = holdCategories
    ? new Promise((resolve) => (releaseCategories = resolve))
    : null;
  const api = {
    categories: CATEGORIES,
    releaseCategories: () => releaseCategories(),
    posts: [],
    indexes: [],
    presigns: 0,
    attaches: 0,
    uploads: 0,
  };
  await desktopPage(page, {}, { windowLabel: 'quick-add' });
  await page.addInitScript((png) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
    const internals = window.__TAURI_INTERNALS__;
    const base = internals.invoke;
    const quick = {
      queue: [{ workspaceId: 1 }],
      calls: [],
      capture: null,
      png: Uint8Array.from(atob(png), (c) => c.charCodeAt(0)),
    };
    window.__quick = quick;
    internals.invoke = (command, args, options) => {
      switch (command) {
        case 'take_quick_add':
          return Promise.resolve(quick.queue.shift() ?? null);
        case 'take_capture':
          return Promise.resolve(quick.png);
        case 'hide_quick_add':
        case 'capture_screenshot':
          quick.calls.push({ command });
          return Promise.resolve(
            command === 'capture_screenshot' ? quick.capture : null,
          );
        case 'open_quick_add':
          quick.calls.push({ command, args });
          quick.queue.push(args.payload ?? null);
          setTimeout(() => window.__emit('quick-add://open'), 0);
          return Promise.resolve(null);
        default:
          return base(command, args, options);
      }
    };
  }, PNG);
  if (remembered !== null)
    await page.addInitScript(
      (value) => localStorage.setItem('desktop.quickAdd.category.1', value),
      remembered,
    );
  await page.route('**/__storage/**', async (route) => {
    api.uploads++;
    await route.fulfill({ status: 200, body: '' });
  });
  await page.route('**/api/**', async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname.replace('/api/', '');
    const method = request.method();
    if (method === 'GET' && path === 'project_categories') {
      if (failCategories)
        return route.fulfill({ status: 500, json: { message: 'down' } });
      await categoriesGate;
      return route.fulfill({ json: { data: api.categories } });
    }
    if (method === 'GET' && path === 'tasks/indexes') {
      api.indexes.push(new URL(request.url()).search);
      if (failIndexes)
        return route.fulfill({ status: 500, json: { message: 'down' } });
      return route.fulfill({
        json: { data: { index: { workspace: 90, category: 158 } } },
      });
    }
    if (method === 'POST' && path === 'tasks') {
      api.posts.push({ path, body: request.postDataJSON() });
      return route.fulfill({ json: { data: { id: 501 } } });
    }
    if (method === 'POST' && path === 'daily-routines/tasks') {
      api.posts.push({ path, body: request.postDataJSON() });
      return route.fulfill({ json: { data: { id: 5, workspace_id: 1 } } });
    }
    if (method === 'POST' && path === 'files/presign-upload') {
      api.presigns++;
      return route.fulfill({
        json: {
          data: {
            upload_url: `${new URL(request.url()).origin}/__storage/shot.png`,
            key: 'tmp/shot.png',
            content_type: 'image/png',
          },
        },
      });
    }
    if (method === 'POST' && path === 'tasks/501/files') {
      api.attaches++;
      return route.fulfill({ json: { data: { id: 9 } } });
    }
    return route.fallback();
  });
  return api;
};

const reopen = (page, payload = { workspaceId: 1 }) =>
  page.evaluate((next) => {
    window.__quick.queue.push(next);
    window.__emit('quick-add://open');
  }, payload);

const calls = (page) => page.evaluate(() => window.__quick.calls);
const mode = (page, name) => page.getByTestId(`quick-add-mode-${name}`);
const title = (page) => page.getByPlaceholder('What needs to be done?');
const category = (page) => page.getByTestId('quick-add-category');
const add = (page) => page.getByTestId('quick-add-submit');
const chosen = (page) => category(page).locator('option:checked');

const open = async (page, options) => {
  const api = await setup(page, options);
  await page.goto('/quick-add');
  await expect(mode(page, 'routine')).toBeVisible();
  await expect(page.locator('select option', { hasText: 'Demo' })).toHaveCount(
    1,
  );
  return api;
};

test.describe('quick add type and category', () => {
  test('switches types keeping the text and creates what was chosen', async ({
    page,
  }) => {
    const api = await open(page);
    for (const name of ['Routine', 'Task', 'Task + screenshot', 'Page'])
      await expect(
        page.getByRole('button', { name, exact: true }),
      ).toBeVisible();
    await expect(mode(page, 'routine')).toHaveAttribute('aria-pressed', 'true');
    await expect(category(page)).toHaveCount(0);

    await title(page).fill('Fix login');
    await mode(page, 'task').click();
    await expect(title(page)).toHaveValue('Fix login');
    await expect(category(page)).toBeVisible();
    await expect(category(page).locator('option')).toHaveText([
      'No category',
      'Home',
      'Work',
    ]);
    await mode(page, 'screenshot').click();
    await expect(title(page)).toHaveValue('Fix login');
    await mode(page, 'routine').click();
    await expect(category(page)).toHaveCount(0);

    await add(page).click();
    await expect.poll(() => api.posts.length).toBe(1);
    expect(api.posts[0].path).toBe('daily-routines/tasks');
    expect(api.posts[0].body).toMatchObject({
      title: 'Fix login',
      workspace_id: 1,
    });
    expect(api.posts[0].body.project_category_id).toBeUndefined();
    await expect
      .poll(async () =>
        (await calls(page)).some((c) => c.command === 'hide_quick_add'),
      )
      .toBe(true);
    expect(api.indexes).toEqual([]);
  });

  test('task: remembered category, prefix preview and a single prefix', async ({
    page,
  }) => {
    const api = await open(page);
    await mode(page, 'task').click();
    await expect(chosen(page)).toHaveText('No category');
    await category(page).selectOption({ label: 'Work' });
    await title(page).fill('Fix login');
    await expect(page.getByTestId('quick-add-prefix')).toHaveText('TM-158:');
    expect(api.indexes.at(-1)).toContain('workspace_id=1');
    expect(api.indexes.at(-1)).toContain('category=10');

    await add(page).click();
    await expect.poll(() => api.posts.length).toBe(1);
    expect(api.posts[0].path).toBe('tasks');
    expect(api.posts[0].body).toMatchObject({
      title: 'TM-158: Fix login',
      project_category_id: 10,
      workspace_id: 1,
      status_id: 1,
      is_daily_routine: false,
    });
    expect(api.presigns).toBe(0);
    await expect
      .poll(() =>
        page.evaluate(() =>
          localStorage.getItem('desktop.quickAdd.category.1'),
        ),
      )
      .toBe('10');

    await expect(title(page)).toHaveValue('');
    await expect(mode(page, 'routine')).toHaveAttribute('aria-pressed', 'true');
    await reopen(page);
    await mode(page, 'task').click();
    await expect(chosen(page)).toHaveText('Work');
    await expect(page.getByTestId('quick-add-prefix')).toHaveText('TM-158:');

    await title(page).fill('TM-12: Already numbered');
    await expect(page.getByTestId('quick-add-prefix')).toHaveCount(0);
    await add(page).click();
    await expect.poll(() => api.posts.length).toBe(2);
    expect(api.posts[1].body.title).toBe('TM-12: Already numbered');
  });

  test('a routine never touches the remembered category, No category is remembered', async ({
    page,
  }) => {
    const api = await open(page, { remembered: '10' });
    await mode(page, 'task').click();
    await expect(chosen(page)).toHaveText('Work');
    await category(page).selectOption({ label: 'No category' });
    await title(page).fill('Plain task');
    await add(page).click();
    await expect.poll(() => api.posts.length).toBe(1);
    expect(api.posts[0].body.title).toBe('Plain task');
    expect(api.posts[0].body.project_category_id).toBeUndefined();
    await expect
      .poll(() =>
        page.evaluate(() =>
          localStorage.getItem('desktop.quickAdd.category.1'),
        ),
      )
      .toBe('none');
  });

  test('a stale remembered category falls back to No category', async ({
    page,
  }) => {
    await open(page, { remembered: '999' });
    await mode(page, 'task').click();
    await expect(category(page).locator('option')).toHaveCount(3);
    await expect(chosen(page)).toHaveText('No category');
  });

  test('the draft keeps type and category across Esc, Discard re-selects the remembered one', async ({
    page,
  }) => {
    await open(page, { remembered: '10' });
    await mode(page, 'task').click();
    await expect(chosen(page)).toHaveText('Work');
    await category(page).selectOption({ label: 'Home' });
    await title(page).fill('Half typed');
    await page.keyboard.press('Escape');
    await expect
      .poll(async () =>
        (await calls(page)).some((c) => c.command === 'hide_quick_add'),
      )
      .toBe(true);
    await reopen(page);
    await expect(title(page)).toHaveValue('Half typed');
    await expect(mode(page, 'task')).toHaveAttribute('aria-pressed', 'true');
    await expect(chosen(page)).toHaveText('Home');

    await page.getByTestId('quick-add-discard').click();
    await expect(title(page)).toHaveValue('');
    await expect(mode(page, 'routine')).toHaveAttribute('aria-pressed', 'true');
    await mode(page, 'task').click();
    await expect(chosen(page)).toHaveText('Work');
  });

  test('a screenshot switches to task + screenshot, keeps it unsent in other types', async ({
    page,
  }) => {
    const api = await open(page);
    await title(page).fill('Look at this');
    await reopen(page, { workspaceId: 1, screenshot: '/tmp/a.png' });
    await expect(mode(page, 'screenshot')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await expect(title(page)).toHaveValue('Look at this');
    await expect(page.getByAltText('Screenshot')).toHaveCount(1);

    await mode(page, 'task').click();
    await expect(page.getByAltText('Screenshot')).toHaveCount(0);
    await expect(page.getByTestId('quick-add-unsent-hint')).toContainText(
      'Adding discards it',
    );
    await mode(page, 'screenshot').click();
    await expect(page.getByAltText('Screenshot')).toHaveCount(1);
    await expect(page.getByTestId('quick-add-unsent-hint')).toHaveCount(0);

    await mode(page, 'task').click();
    await add(page).click();
    await expect.poll(() => api.posts.length).toBe(1);
    expect(api.presigns).toBe(0);
    expect(api.attaches).toBe(0);
  });

  test('task + screenshot uploads the screenshot', async ({ page }) => {
    const api = await open(page);
    await reopen(page, { workspaceId: 1, screenshot: '/tmp/a.png' });
    await expect(mode(page, 'screenshot')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await expect(add(page)).toBeDisabled();
    await title(page).fill('Bug in header');
    await add(page).click();
    await expect.poll(() => api.attaches).toBe(1);
    expect(api.posts).toHaveLength(1);
    expect(api.posts[0].path).toBe('tasks');
    expect(api.presigns).toBe(1);
    expect(api.uploads).toBe(1);
  });

  test('empty task + screenshot offers Capture and Choose image', async ({
    page,
  }) => {
    const api = await open(page);
    await mode(page, 'screenshot').click();
    await title(page).fill('Needs a shot');
    await expect(page.getByTestId('quick-add-capture')).toBeVisible();
    await expect(page.getByTestId('quick-add-choose-image')).toBeVisible();
    await expect(
      page.getByText('Capture or choose a screenshot'),
    ).toBeVisible();
    await expect(add(page)).toBeDisabled();

    await page.getByTestId('quick-add-file').setInputFiles({
      name: 'shot.png',
      mimeType: 'image/png',
      buffer: Buffer.from(PNG, 'base64'),
    });
    await expect(page.getByAltText('Screenshot')).toHaveCount(1);
    await expect(add(page)).toBeEnabled();
    await page.getByRole('button', { name: 'Remove' }).click();
    await expect(add(page)).toBeDisabled();

    await page.evaluate(() => (window.__quick.capture = '/tmp/c.png'));
    await page.getByTestId('quick-add-capture').click();
    await expect(page.getByAltText('Screenshot')).toHaveCount(1);
    const sequence = (await calls(page)).map((c) => c.command);
    expect(sequence).toEqual([
      'hide_quick_add',
      'capture_screenshot',
      'open_quick_add',
    ]);
    expect((await calls(page))[2].args.payload).toEqual({
      workspaceId: 1,
      screenshot: '/tmp/c.png',
    });
    await expect(title(page)).toHaveValue('Needs a shot');
    await expect(add(page)).toBeEnabled();
    expect(api.posts).toHaveLength(0);
  });

  test('a cancelled capture brings the window back with the draft', async ({
    page,
  }) => {
    await open(page);
    await mode(page, 'screenshot').click();
    await category(page).selectOption({ label: 'Home' });
    await page.getByTestId('quick-add-capture').click();
    await expect.poll(async () => (await calls(page)).length).toBe(3);
    expect((await calls(page))[2].args.payload).toEqual({ workspaceId: 1 });
    await expect(mode(page, 'screenshot')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await expect(chosen(page)).toHaveText('Home');
    await expect(page.getByTestId('quick-add-capture')).toBeVisible();
  });

  test('categories failing to load leave No category, creating still works', async ({
    page,
  }) => {
    const api = await open(page, { failCategories: true });
    await mode(page, 'task').click();
    await expect(page.getByTestId('quick-add-categories-error')).toBeVisible();
    await expect(category(page).locator('option')).toHaveText(['No category']);
    await title(page).fill('Still works');
    await add(page).click();
    await expect.poll(() => api.posts.length).toBe(1);
    expect(api.posts[0].body.title).toBe('Still works');
    expect(api.posts[0].body.project_category_id).toBeUndefined();
  });

  test('a failing index request stops the submit and keeps the draft', async ({
    page,
  }) => {
    const api = await open(page, { failIndexes: true, remembered: '10' });
    await mode(page, 'task').click();
    await expect(chosen(page)).toHaveText('Work');
    await title(page).fill('Keep me');
    await expect(page.getByTestId('quick-add-prefix')).toHaveCount(0);
    await add(page).click();
    await expect(
      page.getByText('Could not get the next task number'),
    ).toBeVisible();
    expect(api.posts).toHaveLength(0);
    await expect(title(page)).toHaveValue('Keep me');
    await expect(add(page)).toBeEnabled();
  });

  test('a fresh open refetches the categories and drops a deleted one', async ({
    page,
  }) => {
    const api = await open(page, { remembered: '10' });
    await mode(page, 'task').click();
    await expect(chosen(page)).toHaveText('Work');
    await expect(page.getByTestId('quick-add-prefix')).toHaveText('TM-158:');

    api.categories = [
      { ...CATEGORIES[1], title: 'Home sweet home' },
      {
        ...CATEGORIES[0],
        id: 12,
        title: 'Side',
        settings: [{ id: 3, key: PATTERN_KEY, value: 'S-{index#category}' }],
      },
    ];
    await reopen(page);
    await expect(mode(page, 'routine')).toHaveAttribute('aria-pressed', 'true');
    await mode(page, 'task').click();
    await expect(category(page).locator('option')).toHaveText([
      'No category',
      'Home sweet home',
      'Side',
    ]);
    await expect(chosen(page)).toHaveText('No category');
    await expect(page.getByTestId('quick-add-prefix')).toHaveCount(0);
    await category(page).selectOption({ label: 'Side' });
    await expect(page.getByTestId('quick-add-prefix')).toHaveText('S-158:');
  });

  test('a kept draft drops a category that was deleted meanwhile', async ({
    page,
  }) => {
    const api = await open(page, { remembered: '10' });
    await mode(page, 'task').click();
    await title(page).fill('Half typed');
    await expect(chosen(page)).toHaveText('Work');
    api.categories = [CATEGORIES[1]];
    await reopen(page);
    await expect(title(page)).toHaveValue('Half typed');
    await expect(category(page).locator('option')).toHaveText([
      'No category',
      'Home',
    ]);
    await expect(chosen(page)).toHaveText('No category');
  });

  test('a draft parked as a routine drops a deleted category once Task is chosen again', async ({
    page,
  }) => {
    const api = await open(page, { remembered: '10' });
    await mode(page, 'task').click();
    await title(page).fill('Parked');
    await expect(chosen(page)).toHaveText('Work');
    await mode(page, 'routine').click();
    await page.keyboard.press('Escape');
    await expect
      .poll(async () =>
        (await calls(page)).some((c) => c.command === 'hide_quick_add'),
      )
      .toBe(true);
    api.categories = [CATEGORIES[1]];
    await reopen(page);
    await expect(title(page)).toHaveValue('Parked');
    await mode(page, 'task').click();
    await expect(category(page).locator('option')).toHaveText([
      'No category',
      'Home',
    ]);
    await expect(chosen(page)).toHaveText('No category');
  });

  test('a failed category load does not overwrite the remembered one', async ({
    page,
  }) => {
    const api = await open(page, { failCategories: true, remembered: '10' });
    await mode(page, 'task').click();
    await expect(page.getByTestId('quick-add-categories-error')).toBeVisible();
    await title(page).fill('Still works');
    await add(page).click();
    await expect.poll(() => api.posts.length).toBe(1);
    await expect
      .poll(async () =>
        (await calls(page)).some((c) => c.command === 'hide_quick_add'),
      )
      .toBe(true);
    expect(
      await page.evaluate(() =>
        localStorage.getItem('desktop.quickAdd.category.1'),
      ),
    ).toBe('10');
  });

  test('Add waits while the categories load for a task', async ({ page }) => {
    const api = await open(page, { holdCategories: true });
    await title(page).fill('Wait for me');
    await expect(add(page)).toBeEnabled();
    await mode(page, 'task').click();
    await expect(add(page)).toBeDisabled();
    await title(page).press('Enter');
    await page.waitForTimeout(300);
    expect(api.posts).toHaveLength(0);
    api.releaseCategories();
    await expect(add(page)).toBeEnabled();
  });

  test('the compact window fits the header, switch and title', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 620, height: 160 });
    await open(page, { remembered: '10' });
    await mode(page, 'task').click();
    await expect(page.getByTestId('quick-add-prefix')).toHaveText('TM-158:');
    for (const locator of [
      add(page),
      mode(page, 'page'),
      category(page),
      title(page),
      page.getByRole('button', { name: '+ Description' }),
    ]) {
      const box = await locator.boundingBox();
      expect(box.y + box.height).toBeLessThanOrEqual(160);
      expect(box.x + box.width).toBeLessThanOrEqual(620);
    }
  });
});
