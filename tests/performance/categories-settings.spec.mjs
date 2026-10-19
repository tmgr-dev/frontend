import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const PATTERN = 'task_name_pattern_date&time';
const CATALOGUE = [
  {
    id: 1,
    name: 'The date&time pattern',
    key: PATTERN,
    component_type: 'text',
    description: 'The date&time pattern for a task name',
    custom_value_available: true,
    variable_type: 'string',
    rules: 'sometimes|string',
    default_values: [
      { id: 1, value: 'YYYY-MM-DD' },
      { id: 2, value: 'DD.MM.YYYY' },
    ],
  },
  {
    id: 2,
    name: 'Cost per hour',
    key: 'task_cost_per_hour',
    component_type: 'integer',
    description: '',
    custom_value_available: true,
    variable_type: 'string',
    rules: 'sometimes|number',
    default_values: [],
  },
  {
    id: 3,
    name: 'Approximately time',
    key: 'approximately_time',
    component_type: 'time_in_seconds',
    description: '',
    custom_value_available: true,
    variable_type: 'integer',
    rules: 'sometimes|number',
    default_values: [],
  },
  {
    id: 4,
    name: 'Active countdown reminder every',
    key: 'active_countdown_reminder_every',
    component_type: 'time_in_seconds',
    description:
      'Every time after reaching out setted time you will get sound reminder.',
    custom_value_available: true,
    variable_type: 'integer',
    rules: 'sometimes|number',
    default_values: [],
  },
];

const settingsOf = (values) =>
  CATALOGUE.filter(
    (def) => values[def.id] != null && values[def.id] !== '',
  ).map((def) => ({
    id: def.id,
    name: def.name,
    key: def.key,
    description: def.description,
    value: String(values[def.id]),
  }));

const cat = (
  id,
  title,
  code,
  parent,
  { pat, rate, est, rem, cursor = false } = {},
) => ({
  id,
  title,
  code,
  slug: code.toLowerCase(),
  project_category_id: parent,
  workspace_id: 1,
  user_id: 1,
  cursor_connected: cursor,
  deleted_at: null,
  settings: settingsOf({ 1: pat, 2: rate, 3: est, 4: rem }),
});

const DATE_DAY = '{dts#DD.MM.YYYY ddd#dte}';
const FULL = { rate: 700, est: 28800, rem: 3600 };

const NESTED = () => [
  cat(24, 'Work', 'WORK', null),
  cat(101, 'Cove', 'COVE', 24, { pat: DATE_DAY, ...FULL }),
  cat(27, 'Freelance', 'FREELANCE', 24, { pat: 'DD.MM.YYYY' }),
  cat(151, 'FCPP', 'FCPP', 27, { pat: DATE_DAY, ...FULL }),
  cat(123, 'suncityarena', 'SUNCITYARENA', 27, { pat: 'DD.MM.YYYY', ...FULL }),
  cat(122, 'Morepoke', 'MOREPOKE', 27, { pat: 'DD.MM.YYYY', ...FULL }),
  cat(94, 'If-so.com', 'IFSOCOM', 27, { pat: 'DD.MM.YYYY', ...FULL, rate: 20 }),
  cat(26, 'Onza', 'ONZA', 24, { pat: 'DD.MM.YYYY' }),
  cat(91, 'Wowmii', 'WOWMII', 26, { pat: 'DD.MM.YYYY', est: 28800, rem: 3600 }),
  cat(50, 'Some stuff', 'SOMESTUFF', null),
  cat(175, 'Foodbase', 'FOODBASE', 50, {
    pat: DATE_DAY,
    rate: 25,
    est: 28800,
    rem: 3600,
  }),
  cat(47, 'Common projects', 'COMMONPROJECTS', null, {
    pat: 'DD.MM.YYYY ddd',
    est: 18000,
  }),
  cat(48, 'Task Manager', 'TASKMANAGER', 47, { pat: DATE_DAY }),
];

const FLAT = () => [
  cat(127, 'TM', 'TM', null, {
    pat: 'TM-{index#category}',
    rate: 123,
    est: 14400,
    rem: 1200,
    cursor: true,
  }),
  cat(179, 'TMBE', 'TMBE', null, {
    pat: 'TMBE-{index#category}',
    rate: 56,
    cursor: true,
  }),
  cat(165, 'Fixes', 'FIXES', null, { rate: 56 }),
  cat(148, 'TC', 'TC', null, {
    pat: 'TC-{index#category}:',
    rate: 12,
    est: 10800,
  }),
  cat(147, 'TEST', 'TEST', null, { pat: 'Test-{index#category}:' }),
  cat(126, 'TMGR-Old', 'TMGROLD', null, {
    pat: 'TMGR-{index#workspace}:',
    rate: 123,
    est: 14400,
    rem: 1200,
  }),
];

const LITERAL_IDS = [27, 123, 122, 94, 26, 91, 47];

const boot = async (
  page,
  { list = NESTED(), workspace = 'Personal', scheme, size } = {},
) => {
  if (size) await page.setViewportSize(size);
  await page.clock.setFixedTime(new Date('2026-10-12T10:00:00'));
  await page.addInitScript((dark) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    if (dark) localStorage.setItem('colorScheme', 'dark');
  }, scheme === 'dark');
  if (scheme) await page.emulateMedia({ colorScheme: scheme });
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await mockApp(page);
  await page.route('**/api/workspaces', (route) =>
    route.fulfill({
      json: {
        data: [
          {
            id: 1,
            name: workspace,
            code: 'demo',
            user_id: 1,
            is_default: true,
          },
        ],
      },
    }),
  );
  await page.route(
    (u) =>
      u.pathname.startsWith('/api/') &&
      u.pathname.endsWith('feature-toggles') &&
      !u.pathname.includes('/user'),
    (route) =>
      route.fulfill({
        json: {
          data: Object.fromEntries(
            ['board', 'categories', 'cursor'].map((key) => [
              key,
              {
                key,
                name: key,
                group: 'pages',
                type: 'boolean',
                enabled: true,
              },
            ]),
          ),
        },
      }),
  );
  const state = { list, calls: [], nextId: 900, fail: {} };
  await page.route('**/api/project_categories**', async (route) => {
    const request = route.request();
    const method = request.method();
    const path = new URL(request.url()).pathname.replace(/^.*\/api\//, '');
    const body =
      method === 'GET' || method === 'DELETE' ? null : request.postDataJSON();
    if (method !== 'GET') state.calls.push({ method, path, body });
    const find = (id) => state.list.find((c) => c.id === Number(id));
    const failure = state.fail[`${method} ${path}`];
    if (failure)
      return route.fulfill({ status: failure.status, json: failure.json });
    if (method === 'GET' && path === 'project_categories')
      return route.fulfill({ json: { data: state.list } });
    if (method === 'GET' && path === 'project_categories/settings')
      return route.fulfill({ json: { data: CATALOGUE } });
    if (method === 'POST' && path === 'project_categories') {
      const created = {
        ...cat(
          state.nextId++,
          body.title,
          body.code || 'NEW',
          body.project_category_id ?? null,
        ),
        slug: body.slug,
      };
      state.list = [...state.list, created];
      return route.fulfill({ status: 201, json: { data: created } });
    }
    let m = path.match(/^project_categories\/(\d+)\/settings$/);
    if (m && method === 'PUT') {
      const target = find(m[1]);
      target.settings = settingsOf(
        Object.fromEntries(body.map((s) => [s.id, s.value])),
      );
      return route.fulfill({ json: { data: { ...target } } });
    }
    m = path.match(/^project_categories\/(\d+)$/);
    if (m && method === 'PUT') {
      Object.assign(find(m[1]), {
        title: body.title,
        code: body.code,
        project_category_id: body.project_category_id,
      });
      return route.fulfill({ json: { data: { ...find(m[1]) } } });
    }
    if (m && method === 'DELETE') {
      const target = find(m[1]);
      target.deleted_at = '2026-10-12T10:00:00Z';
      return route.fulfill({ json: { data: { ...target } } });
    }
    return route.fulfill({ json: { data: [] } });
  });
  return { state, errors };
};

const DESK = { width: 1440, height: 900 };
const LITERAL = 'Title pattern has no placeholders';
const escapeRe = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const linkName = (name) => new RegExp(`^${escapeRe(name)}(\\s|$)`);
const sidebar = (page) => page.getByTestId('categories-sidebar-section');
const treeRow = (page, name) =>
  sidebar(page).getByRole('link', { name: linkName(name) });
const expand = (page, name) =>
  sidebar(page)
    .locator('li')
    .filter({ has: page.getByRole('link', { name: linkName(name) }) })
    .getByRole('button', { name: 'Expand' })
    .click();
const header = (page) => page.locator('[data-page-header]');
const tabs = (page) => page.getByRole('navigation', { name: 'Category' });
const bar = (page) => page.getByText(/^\d+ unsaved changes?$/);
const nameField = (page) =>
  page.getByRole('textbox', { name: 'Name', exact: true });
const preview = (page) =>
  page.getByText('Preview', { exact: true }).locator('..');
const patternField = (page) =>
  page.getByRole('textbox', { name: 'Task title pattern' });
const writes = (state, re) =>
  state.calls.filter((c) => re.test(`${c.method} ${c.path}`));
const noHorizontalScroll = (page) =>
  page.evaluate(
    () =>
      document.documentElement.scrollWidth <=
      document.documentElement.clientWidth,
  );

test.describe.configure({ timeout: 90_000 });

test('index opens the first root; the sidebar tree marks literal patterns; filter keeps ancestors; tabs live in the URL', async ({
  page,
}) => {
  const { errors } = await boot(page, { size: DESK });
  await page.goto('/demo/categories');
  await expect(page).toHaveURL(/\/demo\/categories\/47\/children$/);
  await expect(header(page).locator('h1')).toHaveText('Common projects');
  await expect(header(page).locator('p')).toHaveText('Top level');
  await expect(sidebar(page).getByRole('link')).toHaveCount(3);
  await expect(treeRow(page, 'Common projects')).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(
    treeRow(page, 'Common projects').getByRole('img', { name: LITERAL }),
  ).toBeVisible();

  await expand(page, 'Work');
  await expect(sidebar(page).getByRole('link')).toHaveCount(6);
  await expect(
    treeRow(page, 'Freelance').getByRole('img', { name: LITERAL }),
  ).toBeVisible();
  await expect(
    treeRow(page, 'Cove').getByRole('img', { name: LITERAL }),
  ).toHaveCount(0);

  await sidebar(page)
    .getByRole('button', { name: 'Filter categories' })
    .click();
  const filter = sidebar(page).getByRole('searchbox', {
    name: 'Filter categories',
  });
  await expect(filter).toBeFocused();
  await filter.fill('fcpp');
  await expect(sidebar(page).getByRole('link')).toHaveCount(3);
  await expect(treeRow(page, 'Freelance')).toBeVisible();
  await expect(treeRow(page, 'FCPP')).toBeVisible();
  await filter.press('Escape');
  await expect(filter).toBeHidden();
  await expect(sidebar(page).getByRole('link')).toHaveCount(6);

  await treeRow(page, 'Cove').click();
  await expect(page).toHaveURL(/\/demo\/categories\/101\/children$/);
  await expect(header(page).locator('h1')).toHaveText('Cove');
  await expect(header(page).locator('p')).toHaveText('Work');
  await expect(treeRow(page, 'Cove')).toHaveAttribute('aria-current', 'page');
  await expect(tabs(page).getByRole('link', { name: 'Tasks' })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await tabs(page).getByRole('link', { name: 'Settings' }).click();
  await expect(page).toHaveURL(/\/101\/children\?tab=settings$/);
  await expect(nameField(page)).toHaveValue('Cove');
  await page.reload();
  await expect(
    tabs(page).getByRole('link', { name: 'Settings' }),
  ).toHaveAttribute('aria-current', 'page');
  await expect(nameField(page)).toHaveValue('Cove');
  await page.goBack();
  await expect(page).toHaveURL(/\/101\/children$/);
  await expect(nameField(page)).toHaveCount(0);

  await page.goto('/demo/categories');
  await expect(page).toHaveURL(/\/demo\/categories\/101\/children$/);
  await page.goto('/demo/categories/99999/children');
  await expect(page.getByTestId('category-missing')).toContainText(
    'Category not found',
  );
  await expect(page.getByText(/Could not/)).toHaveCount(0);
  expect(await noHorizontalScroll(page)).toBe(true);
  expect(errors).toEqual([]);
});

test('flat workspace: Cursor marks, no literal notice, durations read as durations', async ({
  page,
}) => {
  const { errors } = await boot(page, {
    list: FLAT(),
    workspace: 'TMGR.DEV',
    size: DESK,
  });
  await page.goto('/demo/categories');
  await expect(page).toHaveURL(/\/demo\/categories\/165\/children$/);
  await expect(sidebar(page).getByRole('link')).toHaveCount(6);
  await expect(
    sidebar(page).getByRole('img', { name: 'Cursor connected' }),
  ).toHaveCount(2);
  await expect(sidebar(page).getByRole('img', { name: LITERAL })).toHaveCount(
    0,
  );

  await treeRow(page, 'TM').click();
  await tabs(page).getByRole('link', { name: 'Settings' }).click();
  await expect(page).toHaveURL(/\/127\/children\?tab=settings$/);
  await expect(page.getByText(/new tasks literally/)).toHaveCount(0);
  await expect(preview(page)).toContainText('TM-#: Task title');
  await expect(
    page.getByRole('textbox', { name: 'Rate per hour' }),
  ).toHaveValue('123');
  await expect(
    page.getByRole('textbox', { name: 'Default estimate' }),
  ).toHaveValue('4h');
  await expect(
    page.getByRole('textbox', { name: 'Timer reminder every' }),
  ).toHaveValue('20m');
  await expect(page.getByText('Cursor AI agent')).toBeVisible();
  await expect(page.getByText('Connected', { exact: true })).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Manage', exact: true }),
  ).toBeVisible();
  await expect(bar(page)).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('durations: invalid text blocks Save; a valid one is saved as seconds', async ({
  page,
}) => {
  const { state, errors } = await boot(page, { list: FLAT(), size: DESK });
  await page.goto('/demo/categories/127/children?tab=settings');
  const estimate = page.getByRole('textbox', { name: 'Default estimate' });
  const reminder = page.getByRole('textbox', { name: 'Timer reminder every' });
  const error = page.getByText('Use a duration like 45m, 2h or 1h 30m');
  const save = page.getByRole('button', { name: 'Save', exact: true });

  await estimate.fill('abc');
  await estimate.blur();
  await expect(error).toBeVisible();
  await expect(estimate).toHaveAttribute('aria-invalid', 'true');
  await expect(bar(page)).toHaveText('1 unsaved change');
  await expect(save).toBeDisabled();

  await estimate.fill('1h 30m');
  await reminder.fill('90');
  await reminder.blur();
  await expect(error).toHaveCount(0);
  await expect(bar(page)).toHaveText('2 unsaved changes');
  await expect(save).toBeEnabled();
  await save.click();
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  expect(
    writes(state, /^PUT project_categories\/127\/settings$/)[0].body,
  ).toEqual([
    { id: 1, value: 'TM-{index#category}' },
    { id: 2, value: '123' },
    { id: 3, value: '5400' },
    { id: 4, value: '5400' },
  ]);
  await expect(bar(page)).toHaveCount(0);
  await expect(estimate).toHaveValue('1h 30m');
  await expect(reminder).toHaveValue('1h 30m');
  expect(errors).toEqual([]);
});

test('literal pattern: the fix button rewrites the field without saving; chips insert at the cursor', async ({
  page,
}) => {
  const { state, errors } = await boot(page, { size: DESK });
  await page.goto('/demo/categories/123/children?tab=settings');
  const field = patternField(page);
  await expect(field).toHaveValue('DD.MM.YYYY');
  await expect(field).toHaveAttribute('aria-invalid', 'true');
  await expect(
    page.getByText(
      /No placeholders\. Every new task will literally start with “DD\.MM\.YYYY”/,
    ),
  ).toBeVisible();
  await expect(preview(page)).toContainText('DD.MM.YYYY: Task title');
  await expect(bar(page)).toHaveCount(0);

  await page.getByRole('button', { name: 'Use {dts#dd.mm.yyyy#dte}' }).click();
  await expect(field).toHaveValue('{dts#dd.mm.yyyy#dte}');
  await expect(field).not.toHaveAttribute('aria-invalid', 'true');
  await expect(preview(page)).toContainText('12.10.2026: Task title');
  await expect(bar(page)).toHaveText('1 unsaved change');
  await page.waitForTimeout(300);
  expect(writes(state, /^PUT /)).toEqual([]);

  await field.evaluate((el) => el.setSelectionRange(0, 0));
  await page.getByRole('button', { name: '+ category number' }).click();
  await expect(field).toHaveValue('{index#category}{dts#dd.mm.yyyy#dte}');

  await page.getByRole('button', { name: 'Discard', exact: true }).click();
  await expect(field).toHaveValue('DD.MM.YYYY');
  await expect(bar(page)).toHaveCount(0);
  expect(writes(state, /^PUT /)).toEqual([]);
  expect(errors).toEqual([]);
});

test('typing words before a date token keeps the preview working', async ({
  page,
}) => {
  const { errors } = await boot(page, { list: FLAT(), size: DESK });
  await page.goto('/demo/categories/127/children?tab=settings');
  await patternField(page).fill('Sprint {dts#dd#dte}');
  await expect(preview(page)).toContainText('Sprint 12: Task title');
  await expect(patternField(page)).toBeVisible();
  expect(errors).toEqual([]);
});

test('a stored pattern with words before a date token does not break the tree or the preview', async ({
  page,
}) => {
  const { errors } = await boot(page, {
    list: [
      ...FLAT(),
      cat(300, 'Reports', 'REP', null, { pat: 'Report {dts#dd.mm#dte}' }),
    ],
    size: DESK,
  });
  await page.goto('/demo/categories/300/children?tab=settings');
  await expect(treeRow(page, 'Reports')).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(
    treeRow(page, 'Reports').getByRole('img', { name: LITERAL }),
  ).toHaveCount(0);
  await expect(preview(page)).toContainText('Report 12.10: Task title');
  expect(errors).toEqual([]);
});

test('Fix all rewrites every literal pattern; a partial failure reports the count', async ({
  page,
}) => {
  const { state, errors } = await boot(page, { size: DESK });
  state.fail['PUT project_categories/94/settings'] = {
    status: 400,
    json: { message: 'Nope' },
  };
  await page.goto('/demo/categories/101/children?tab=settings');
  await expect(
    page.getByText('7 categories name new tasks literally'),
  ).toBeVisible();
  await expect(sidebar(page).getByRole('img', { name: LITERAL })).toHaveCount(
    3,
  );
  await page.getByRole('button', { name: 'Fix all 7' }).click();
  await expect(
    page.getByText('Fixed 6 of 7 title patterns', { exact: true }),
  ).toBeVisible();
  const puts = writes(state, /^PUT project_categories\/\d+\/settings$/);
  expect(puts.map((c) => Number(c.path.split('/')[1])).sort()).toEqual(
    [...LITERAL_IDS].sort(),
  );
  expect(
    puts.find((c) => c.path === 'project_categories/123/settings').body,
  ).toEqual([
    { id: 1, value: '{dts#dd.mm.yyyy#dte}' },
    { id: 2, value: '700' },
    { id: 3, value: '28800' },
    { id: 4, value: '3600' },
  ]);
  await expect(
    page.getByText('1 category names new tasks literally'),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Fix all 1' })).toBeVisible();
  await expect(sidebar(page).getByRole('img', { name: LITERAL })).toHaveCount(
    0,
  );
  await expect(bar(page)).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('create at the top level and a subcategory: sidebar +, header + Subcategory, draft row, copy default, land on the new Settings tab', async ({
  page,
}) => {
  const { state, errors } = await boot(page, { size: DESK });
  await page.goto('/demo/categories');
  await expect(page).toHaveURL(/\/demo\/categories\/47\/children$/);
  const plus = sidebar(page).getByRole('button', { name: 'New category' });
  await expect(plus).toHaveAttribute('data-tour', 'categories.create');
  await plus.click();
  await expect(page).toHaveURL(/\/demo\/categories\/create$/);
  await expect(header(page).locator('h1')).toHaveText('New category');
  await expect(header(page).locator('p')).toHaveText('Top level');
  const draft = sidebar(page).locator('[aria-current="page"]');
  await expect(draft).toHaveText('New category');
  await expect(draft).toHaveCSS('font-style', 'italic');
  await expect(tabs(page)).toHaveCount(0);
  await expect(nameField(page)).toBeFocused();
  await expect(page.getByText('New top-level category')).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Cancel', exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Create', exact: true }),
  ).toBeVisible();
  await expect(page.getByText('Copy settings from')).toHaveCount(0);
  await expect(
    page.getByText(
      'Integrations and the danger zone appear after the category is created.',
    ),
  ).toBeVisible();
  await expect(page.getByText('Danger zone', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page).toHaveURL(/\/demo\/categories\/47\/children$/);

  await page.goto('/demo/categories/27/children');
  await header(page).getByRole('button', { name: 'Subcategory' }).click();
  await expect(page).toHaveURL(/\/demo\/categories\/27\/create$/);
  await expect(draft).toHaveText('New category');
  await expect(
    sidebar(page)
      .locator('li')
      .filter({ has: page.locator('[aria-current="page"]') })
      .locator('xpath=preceding-sibling::li[1]'),
  ).toContainText('Freelance');
  await expect(header(page).locator('p')).toHaveText('Work / Freelance');
  await expect(nameField(page)).toBeFocused();
  await expect(page.getByRole('combobox', { name: 'Parent' })).toHaveText(
    'Freelance',
  );
  await expect(
    page.getByRole('combobox', { name: 'Copy settings from' }),
  ).toHaveText('FCPP');
  await expect(patternField(page)).toHaveValue(DATE_DAY);
  await expect(page.getByText('New subcategory of Freelance')).toBeVisible();

  await nameField(page).fill('Newco');
  await page.getByPlaceholder('e.g., PROJ, FEAT, BUG').fill('new co');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page).toHaveURL(
    /\/demo\/categories\/900\/children\?tab=settings$/,
  );
  await expect(header(page).locator('h1')).toHaveText('Newco');
  await expect(header(page).locator('p')).toHaveText('Work / Freelance');
  await expect(treeRow(page, 'Newco')).toHaveAttribute('aria-current', 'page');
  await expect(nameField(page)).toHaveValue('Newco');
  const post = writes(state, /^POST project_categories$/)[0].body;
  expect(post).toMatchObject({
    title: 'Newco',
    code: 'NEW-CO',
    project_category_id: 27,
  });
  expect(
    writes(state, /^PUT project_categories\/900\/settings$/)[0].body,
  ).toEqual([
    { id: 1, value: DATE_DAY },
    { id: 2, value: '700' },
    { id: 3, value: '28800' },
    { id: 4, value: '3600' },
  ]);
  expect(errors).toEqual([]);
});

test('a dirty form asks before switching tab, another tree row, starting a create or going back', async ({
  page,
}) => {
  const { errors } = await boot(page, { size: DESK });
  await page.goto('/demo/categories/101/children');
  await tabs(page).getByRole('link', { name: 'Settings' }).click();
  await nameField(page).fill('Cove 2');
  await expect(bar(page)).toHaveText('1 unsaved change');
  const dialog = page.getByRole('alertdialog');
  const keep = async () => {
    await expect(dialog).toContainText('Discard unsaved changes?');
    await dialog.getByRole('button', { name: 'Keep editing' }).click();
    await expect(dialog).toBeHidden();
    await expect(page).toHaveURL(/\/101\/children\?tab=settings$/);
    await expect(nameField(page)).toHaveValue('Cove 2');
  };

  await treeRow(page, 'Freelance').click();
  await keep();
  await tabs(page).getByRole('link', { name: 'Tasks' }).click();
  await keep();
  await sidebar(page).getByRole('button', { name: 'New category' }).click();
  await keep();
  await page.goBack();
  await keep();

  await treeRow(page, 'Freelance').click();
  await dialog.getByRole('button', { name: 'Discard' }).click();
  await expect(page).toHaveURL(/\/demo\/categories\/27\/children$/);
  await expect(header(page).locator('h1')).toHaveText('Freelance');
  expect(errors).toEqual([]);
});

test('save sends the category and its full settings, then the header and the tree show the new values', async ({
  page,
}) => {
  const { state, errors } = await boot(page, { size: DESK });
  await page.goto('/demo/categories/101/children?tab=settings');
  await nameField(page).fill('Cove Ltd');
  await page.getByPlaceholder('—').fill('800');
  await expect(bar(page)).toHaveText('2 unsaved changes');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  expect(writes(state, /^PUT project_categories\/101$/)[0].body).toMatchObject({
    id: 101,
    title: 'Cove Ltd',
    code: 'COVE',
    project_category_id: 24,
  });
  expect(
    writes(state, /^PUT project_categories\/101\/settings$/)[0].body,
  ).toEqual([
    { id: 1, value: DATE_DAY },
    { id: 2, value: '800' },
    { id: 3, value: '28800' },
    { id: 4, value: '3600' },
  ]);
  await expect(bar(page)).toHaveCount(0);
  await expect(header(page).locator('h1')).toHaveText('Cove Ltd');
  await expect(treeRow(page, 'Cove Ltd')).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(page.getByPlaceholder('—')).toHaveValue('800');
  expect(errors).toEqual([]);
});

test('a 422 shows inline errors and keeps the form dirty; delete goes to the parent, or to the first root', async ({
  page,
}) => {
  const { state, errors } = await boot(page, { size: DESK });
  state.fail['PUT project_categories/101'] = {
    status: 422,
    json: {
      message: 'The given data was invalid.',
      errors: { title: ['The title has already been taken.'] },
    },
  };
  await page.goto('/demo/categories/101/children?tab=settings');
  await nameField(page).fill('FCPP');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(
    page.getByText('The title has already been taken.'),
  ).toBeVisible();
  await expect(nameField(page)).toHaveAttribute('aria-invalid', 'true');
  await expect(bar(page)).toHaveText('1 unsaved change');
  await expect(nameField(page)).toHaveValue('FCPP');
  expect(writes(state, /settings$/)).toEqual([]);

  await page.getByRole('button', { name: 'Discard', exact: true }).click();
  await expect(bar(page)).toHaveCount(0);
  await page.getByRole('button', { name: 'Delete…' }).click();
  await page
    .getByRole('alertdialog')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(page).toHaveURL(/\/demo\/categories\/24\/children$/);
  await expect(header(page).locator('h1')).toHaveText('Work');
  await expect(treeRow(page, 'Cove')).toHaveCount(0);

  await page.goto('/demo/categories/50/children?tab=settings');
  await page.getByRole('button', { name: 'Delete…' }).click();
  await page
    .getByRole('alertdialog')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(page).toHaveURL(/\/demo\/categories\/47\/children$/);
  expect(writes(state, /^DELETE /).map((c) => c.path)).toEqual([
    'project_categories/101',
    'project_categories/50',
  ]);
  expect(errors).toEqual([]);
});

test('empty workspace: one centred call to action, the sidebar section shows only its header', async ({
  page,
}) => {
  const { errors } = await boot(page, { list: [], size: DESK });
  await page.goto('/demo/categories');
  const empty = page.getByTestId('category-empty');
  await expect(
    empty.getByRole('heading', { name: 'No categories yet' }),
  ).toBeVisible();
  await expect(empty).toHaveCSS('text-align', 'center');
  await expect(sidebar(page).getByRole('link')).toHaveCount(0);
  await expect(
    sidebar(page).getByRole('button', { name: 'New category' }),
  ).toBeVisible();
  await expect(tabs(page)).toHaveCount(0);
  await empty.getByRole('button', { name: 'New category' }).click();
  await expect(page).toHaveURL(/\/demo\/categories\/create$/);
  await expect(header(page).locator('h1')).toHaveText('New category');
  expect(errors).toEqual([]);
});

test('old category URLs land on the right page', async ({ page }) => {
  await boot(page, { size: DESK });
  for (const [from, to] of [
    ['/demo/categories/status/x', /\/demo\/categories\/47\/children$/],
    ['/demo/categories/create', /\/demo\/categories\/create$/],
    [
      '/demo/categories/101',
      /\/demo\/categories\/101\/children\?tab=settings$/,
    ],
    ['/demo/categories/27/create', /\/demo\/categories\/27\/create$/],
    [
      '/demo/categories?category=101',
      /\/demo\/categories\/101\/children\?tab=settings$/,
    ],
    ['/demo/categories?create=top', /\/demo\/categories\/create$/],
    ['/demo/categories?create=27', /\/demo\/categories\/27\/create$/],
    ['/demo/WORK', /\/demo\/categories\/24\/children$/],
  ]) {
    await page.goto(from);
    await expect(page, from).toHaveURL(to);
  }
  await expect(header(page).locator('h1')).toHaveText('Work');
  await page.goto('/demo/NOPE');
  await expect(page.getByTestId('category-missing')).toContainText(
    'Category not found',
  );
});

test('phone: tree in the sidebar drawer, no page-level horizontal scroll, full-width save bar', async ({
  page,
}) => {
  const { errors } = await boot(page, { size: { width: 390, height: 844 } });
  await page.goto('/demo/categories/101/children?tab=settings');
  await expect(header(page).locator('h1')).toHaveText('Cove');
  await expect(tabs(page)).toBeVisible();
  expect(await noHorizontalScroll(page)).toBe(true);

  await nameField(page).fill('Cove 2');
  await expect(bar(page)).toBeVisible();
  const box = await bar(page).locator('..').boundingBox();
  expect(box.width).toBeGreaterThanOrEqual(390 - 48);
  expect(box.x + box.width).toBeLessThanOrEqual(390);
  expect(await noHorizontalScroll(page)).toBe(true);
  await page.getByRole('button', { name: 'Discard', exact: true }).click();
  await expect(bar(page)).toHaveCount(0);

  await page.locator('[data-tour="nav.menu-button"]').click();
  await expect(treeRow(page, 'Cove')).toBeVisible();
  await expect(treeRow(page, 'Cove')).toHaveAttribute('aria-current', 'page');
  await treeRow(page, 'Freelance').click();
  await expect(page).toHaveURL(/\/demo\/categories\/27\/children$/);
  await expect(header(page).locator('h1')).toHaveText('Freelance');
  expect(await noHorizontalScroll(page)).toBe(true);
  expect(errors).toEqual([]);
});

const SHOTS = process.env.CATEGORIES_SHOTS;
const STATES = [
  ['Tasks', '/demo/categories/101/children', NESTED, 'Personal'],
  ['Edit', '/demo/categories/101/children?tab=settings', NESTED, 'Personal'],
  ['Warn', '/demo/categories/123/children?tab=settings', NESTED, 'Personal'],
  ['Create', '/demo/categories/27/create', NESTED, 'Personal'],
  ['Flat', '/demo/categories/127/children?tab=settings', FLAT, 'TMGR.DEV'],
  ['Empty', '/demo/categories', () => [], 'Personal'],
];

for (const [state, url, list, workspace] of STATES) {
  for (const width of [1440, 390]) {
    for (const scheme of ['light', 'dark']) {
      test(`screenshot ${state} @${width} ${scheme}`, async ({ page }) => {
        test.skip(
          !SHOTS,
          'set CATEGORIES_SHOTS=<dir> to take the review screenshots',
        );
        const size =
          width === 1440 ? { width, height: 900 } : { width, height: 844 };
        await boot(page, { list: list(), workspace, scheme, size });
        await page.goto(url);
        await expect(
          page
            .locator(
              '[data-page-header] h1:visible, [data-testid="category-empty"] h1:visible',
            )
            .first(),
        ).toBeVisible();
        await page.waitForLoadState('networkidle');
        await page.waitForTimeout(500);
        const name = width === 1440 ? `R4-${state}` : `R4-${state}-390`;
        await page.screenshot({ path: `${SHOTS}/${name}-${scheme}.png` });
      });
    }
  }
}
