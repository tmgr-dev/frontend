import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

// TM-714: priority picker in the task form and badges on board cards / list rows. Medium, null and
// unknown values show no badge (D-001, D-002).

const shotsDir = process.env.SHOTS_DIR;
const screenshot = async (page, name) => {
  if (shotsDir)
    await page.screenshot({
      path: `${shotsDir}/${name}.png`,
      animations: 'disabled',
    });
};

const user = { id: 1, name: 'Test User', email: 'test@example.test' };
const base = {
  description: null,
  description_json: null,
  status: 'created',
  status_id: 1,
  workspace_id: 1,
  user_id: 1,
  user,
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
const VALUES = ['urgent', 'high', 'medium', 'low', null, 'weird'];
const OPEN_LISTBOX = '[role="listbox"][data-state="open"]';

// A stateful backend: list/board/task GETs return the stored tasks, PUT merges into them.
const setup = async (page, { dark = false, values = VALUES } = {}) => {
  await page.addInitScript((d) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    if (d) localStorage.setItem('colorScheme', 'dark');
  }, dark);
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await mockApp(page);
  const tasks = values.map((priority, i) => ({
    ...base,
    id: i + 1,
    title: `Task ${priority ?? 'null'}`,
    ...(priority === undefined ? {} : { priority }),
  }));
  const puts = [];
  const posts = [];
  await page.route('**/api/tasks/status/**', (route) =>
    route.fulfill({ json: { data: tasks } }),
  );
  await page.route('**/api/tasks/current**', (route) =>
    route.fulfill({
      json: {
        data: tasks,
        meta: {
          current_page: 1,
          per_page: 10,
          total: tasks.length,
          last_page: 1,
          from: 1,
          to: tasks.length,
        },
      },
    }),
  );
  await page.route(
    (url) => /^\/api\/tasks\/\d+$/.test(url.pathname),
    async (route) => {
      const request = route.request();
      const id = Number(new URL(request.url()).pathname.split('/').pop());
      const task = tasks.find((t) => t.id === id);
      if (!task || !['GET', 'PUT'].includes(request.method()))
        return route.fallback();
      if (request.method() === 'PUT') {
        const body = request.postDataJSON();
        puts.push(body);
        Object.assign(task, body, { id });
      }
      await route.fulfill({ json: { data: task } });
    },
  );
  await page.route(
    (url) => url.pathname === '/api/tasks',
    async (route) => {
      if (route.request().method() !== 'POST') return route.fallback();
      const body = route.request().postDataJSON();
      posts.push(body);
      await route.fulfill({ json: { data: { ...base, ...body, id: 42 } } });
    },
  );
  return { tasks, puts, posts, errors };
};

const store = (page, read) =>
  page.evaluate(
    (fn) =>
      new Function('s', `return (${fn})(s)`)(
        document.querySelector('#app').__vue_app__.config.globalProperties
          .$store.state,
      ),
    read.toString(),
  );

const expectBadges = async (scope) => {
  await expect(scope.getByText('Task weird').first()).toBeVisible({
    timeout: 60000,
  });
  const badges = scope.getByTestId('task-priority-badge');
  await expect(badges).toHaveCount(3);
  for (const [value, label] of [
    ['urgent', 'Urgent'],
    ['high', 'High'],
    ['low', 'Low'],
  ]) {
    const badge = scope.locator(`[data-priority="${value}"]`);
    await expect(badge).toHaveCount(1);
    await expect(badge).toHaveAttribute('title', `${label} priority`);
    await expect(badge).toHaveText(`${label} priority`);
    await expect(badge.locator('svg')).toBeVisible();
  }
  await expect(scope.getByText('Medium priority')).toHaveCount(0);
};

for (const dark of [false, true]) {
  const theme = dark ? 'dark' : 'light';

  test(`board cards show badges for urgent/high/low only (${theme})`, async ({
    page,
  }) => {
    test.setTimeout(90000);
    await page.setViewportSize({ width: 1280, height: 800 });
    const { errors } = await setup(page, { dark });
    await page.goto('/demo/board');
    await expect(page.locator('html')).toHaveClass(
      dark ? /dark/ : /^(?!.*dark)/,
    );
    await expectBadges(page);
    for (const title of ['Task medium', 'Task null', 'Task weird']) {
      const card = page.locator('[data-task-id]', { hasText: title }).first();
      await expect(card).toBeVisible();
      await expect(card.getByTestId('task-priority-badge')).toHaveCount(0);
    }
    const urgent = page
      .locator('[data-task-id]', { hasText: 'Task urgent' })
      .first();
    await expect(urgent.getByTestId('task-priority-badge')).toHaveCount(1);
    await screenshot(page, `board-${theme}-1280`);
    expect(errors).toEqual([]);
  });

  test(`list rows show badges for urgent/high/low only (${theme})`, async ({
    page,
  }) => {
    test.setTimeout(90000);
    await page.setViewportSize({ width: 1280, height: 800 });
    const { errors } = await setup(page, { dark });
    await page.goto('/demo/list');
    await expectBadges(page);
    const high = page.locator('[data-task-id]', { hasText: 'Task high' });
    await expect(high.first().locator('[data-priority="high"]')).toHaveCount(1);
    await screenshot(page, `list-${theme}-1280`);
    expect(errors).toEqual([]);
  });
}

for (const dark of [false, true])
  for (const route of ['board', 'list']) {
    const theme = dark ? 'dark' : 'light';
    test(`${route} badges on a 390px phone (${theme})`, async ({ page }) => {
      test.setTimeout(90000);
      await page.setViewportSize({ width: 390, height: 844 });
      const { errors } = await setup(page, { dark });
      await page.goto(`/demo/${route}`);
      await expect(
        page.locator('[data-priority="urgent"]').first(),
      ).toBeVisible({
        timeout: 60000,
      });
      await expect(page.getByText('Medium priority')).toHaveCount(0);
      await screenshot(page, `${route}-${theme}-390`);
      expect(errors).toEqual([]);
    });
  }

test('create modal defaults to Medium, picking High posts priority "high"', async ({
  page,
}) => {
  test.setTimeout(90000);
  await page.setViewportSize({ width: 1440, height: 900 });
  const { posts, errors } = await setup(page, { values: ['medium'] });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().waitFor({ timeout: 60000 });
  await page
    .getByRole('button', { name: 'New task', exact: true })
    .first()
    .click();
  await page.getByPlaceholder('Task name').waitFor();
  const picker = page.getByRole('combobox', { name: 'Priority' });
  await expect(picker).toHaveText('Medium');
  await picker.click();
  await expect(page.locator(OPEN_LISTBOX)).toHaveCount(1);
  await page.keyboard.press('Escape');
  expect(
    await store(page, (s) => s.showCreatingTaskModal),
    'create modal still open after Esc on the picker',
  ).toBe(true);
  await expect(page.locator(OPEN_LISTBOX)).toHaveCount(0);
  await picker.click();
  await expect(page.getByRole('option')).toHaveText([
    'Urgent',
    'High',
    'Medium',
    'Low',
  ]);
  await page.getByRole('option', { name: 'High' }).click();
  await expect(picker).toHaveText('High');
  await page.getByPlaceholder('Task name').fill('New prio task');
  await screenshot(page, 'form-create-light');
  await page.keyboard.press('ControlOrMeta+s');
  await expect.poll(() => posts.length, { timeout: 8000 }).toBe(1);
  expect(posts[0].priority).toBe('high');
  expect(errors).toEqual([]);
});

test('create modal saves Medium by default', async ({ page }) => {
  test.setTimeout(90000);
  await page.setViewportSize({ width: 1440, height: 900 });
  const { posts } = await setup(page, { values: ['medium'] });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().waitFor({ timeout: 60000 });
  await page
    .getByRole('button', { name: 'New task', exact: true })
    .first()
    .click();
  await page.getByPlaceholder('Task name').fill('Default prio task');
  await page.keyboard.press('ControlOrMeta+s');
  await expect.poll(() => posts.length, { timeout: 8000 }).toBe(1);
  expect(posts[0].priority).toBe('medium');
});

test('edit: keyboard picks a priority, PUT carries it, reload shows it; Esc closes only the picker', async ({
  page,
}) => {
  test.setTimeout(120000);
  await page.setViewportSize({ width: 1440, height: 900 });
  const { puts, tasks, errors } = await setup(page, { values: ['low'] });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click({ timeout: 60000 });
  await page.getByPlaceholder('Task name').waitFor();
  const picker = page.getByRole('combobox', { name: 'Priority' });
  await expect(picker).toHaveText('Low');

  // Reachable with Tab from the control before it in the properties grid.
  await page.getByTitle('Assign to me').focus();
  await page.keyboard.press('Tab');
  await expect(picker).toBeFocused();

  // Space opens; Esc closes only the listbox, the task modal stays open.
  await page.keyboard.press('Space');
  await expect(page.locator(OPEN_LISTBOX)).toHaveCount(1);
  await page.keyboard.press('Escape');
  expect(
    await store(page, (s) => s.currentTaskIdForModal != null),
    'task modal still open after Esc on the picker',
  ).toBe(true);
  await expect(page.locator(OPEN_LISTBOX)).toHaveCount(0);
  await expect(picker).toBeFocused();
  await expect(picker).toHaveText('Low');
  expect(puts).toEqual([]);

  // Enter opens; arrows move; Enter selects.
  await page.keyboard.press('Enter');
  await expect(page.locator(OPEN_LISTBOX)).toHaveCount(1);
  await expect(page.getByRole('option', { name: 'Low' })).toBeFocused();
  // Radix Select ignores arrow keys for a moment right after opening.
  await page.waitForTimeout(300);
  await page.keyboard.press('ArrowUp');
  await expect(page.getByRole('option', { name: 'Medium' })).toBeFocused();
  await page.keyboard.press('ArrowUp');
  await expect(page.getByRole('option', { name: 'High' })).toBeFocused();
  await page.keyboard.press('ArrowUp');
  await expect(page.getByRole('option', { name: 'Urgent' })).toBeFocused();
  // TM-712: the highlighted option has a background, the selected one is bold.
  const style = (name, prop) =>
    page
      .getByRole('option', { name })
      .evaluate((el, p) => getComputedStyle(el)[p], prop);
  const clear = 'rgba(0, 0, 0, 0)';
  expect(await style('Urgent', 'backgroundColor')).not.toBe(clear);
  expect(await style('High', 'backgroundColor')).toBe(clear);
  expect(await style('Low', 'fontWeight')).toBe('600');
  expect(await style('High', 'fontWeight')).toBe('400');
  await screenshot(page, 'select-highlight-light');
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('option', { name: 'High' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator(OPEN_LISTBOX)).toHaveCount(0);
  await expect(picker).toHaveText('High');
  expect(await store(page, (s) => s.currentTaskIdForModal != null)).toBe(true);

  await expect
    .poll(() => puts.at(-1)?.priority, { timeout: 10000 })
    .toBe('high');
  expect(tasks[0].priority).toBe('high');

  // Reload reopens the task from the URL; the picker reads the stored value.
  await page.reload();
  await expect(page.getByRole('combobox', { name: 'Priority' })).toHaveText(
    'High',
    { timeout: 60000 },
  );
  await page.goto('/demo/list');
  await expect(
    page.locator('[data-task-id="1"] [data-priority="high"]').first(),
  ).toBeVisible({ timeout: 60000 });
  expect(errors).toEqual([]);
});

test('picker in dark theme', async ({ page }) => {
  test.setTimeout(90000);
  await page.setViewportSize({ width: 1280, height: 800 });
  await setup(page, { dark: true, values: ['urgent'] });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click({ timeout: 60000 });
  await expect(page.locator('html')).toHaveClass(/dark/);
  const picker = page.getByRole('combobox', { name: 'Priority' });
  await expect(picker).toHaveText('Urgent');
  await picker.click();
  await expect(page.locator(OPEN_LISTBOX)).toHaveCount(1);
  await screenshot(page, 'form-picker-dark');
});

test('picker on a 390px phone', async ({ page }) => {
  test.setTimeout(90000);
  await page.setViewportSize({ width: 390, height: 844 });
  const { errors } = await setup(page, { values: ['high'] });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click({ timeout: 60000 });
  const picker = page.getByRole('combobox', { name: 'Priority' });
  await expect(picker).toHaveText('High');
  await picker.scrollIntoViewIfNeeded();
  await picker.click();
  await expect(page.getByRole('option')).toHaveCount(4);
  for (const option of await page.getByRole('option').all())
    await expect(option).toBeInViewport();
  await screenshot(page, 'form-picker-light-390');
  expect(errors).toEqual([]);
});
