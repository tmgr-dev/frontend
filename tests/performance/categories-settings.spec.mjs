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

const row = (page, name) =>
  page.getByRole('option', { name: new RegExp(`^${name}\\b`) });
const pane = (page) =>
  page.locator('section').filter({ has: page.locator('form') });
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

test('nothing selected: subtitle, list marks, literal-pattern card; selection lives in the URL', async ({
  page,
}) => {
  const { errors } = await boot(page, { size: { width: 1440, height: 900 } });
  await page.goto('/demo/categories');
  await expect(page.locator('[data-page-header] p')).toHaveText(
    '13 categories · 3 at the top level',
  );
  await expect(
    page.getByRole('listbox', { name: 'Categories' }).getByRole('option'),
  ).toHaveCount(13);
  await expect(page.getByRole('option', { selected: true })).toHaveCount(0);
  await expect(page).toHaveURL(/\/demo\/categories$/);
  await expect(
    page.getByRole('heading', { name: 'Select a category' }),
  ).toBeVisible();
  await expect(
    page.getByText('Its name, parent and defaults for new tasks open here.'),
  ).toBeVisible();

  await expect(row(page, 'Cove')).toContainText('12.10.2026 Mon · 700/h · 8h');
  await expect(row(page, 'suncityarena')).toContainText(
    'DD.MM.YYYY · 700/h · 8h',
  );
  await expect(
    row(page, 'suncityarena').getByLabel('Title pattern has no placeholders'),
  ).toBeVisible();
  await expect(
    row(page, 'Cove').getByLabel('Title pattern has no placeholders'),
  ).toHaveCount(0);
  await expect(row(page, 'Work')).not.toContainText('·');

  await page.keyboard.press('/');
  const filter = page.getByRole('searchbox', { name: 'Filter categories' });
  await expect(filter).toBeFocused();
  await page.keyboard.type('cove');
  await expect(page.getByRole('option')).toHaveCount(2);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('option')).toHaveCount(13);

  const card = page
    .getByText('7 categories name new tasks literally')
    .locator('xpath=ancestor::div[contains(@class,"rounded-xl")][1]');
  await expect(card.getByRole('button', { name: 'Fix all 7' })).toBeVisible();
  await expect(card.getByRole('button', { name: /Open →/ })).toHaveCount(7);
  const issue = card.getByRole('button', { name: /^suncityarena/ });
  await expect(issue).toContainText('Work / Freelance');
  await expect(issue).toContainText('DD.MM.YYYY');
  await issue.click();

  await expect(page).toHaveURL(/\/demo\/categories\?category=123$/);
  await expect(row(page, 'suncityarena')).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(
    page.getByRole('heading', { name: 'suncityarena', level: 2 }),
  ).toBeVisible();
  await expect(pane(page)).toContainText('Work / Freelance /');
  await expect(
    page.locator('a:visible', { hasText: 'Tasks →' }),
  ).toHaveAttribute('href', '/demo/categories/123/children');

  await page.reload();
  await expect(
    page.getByRole('heading', { name: 'suncityarena', level: 2 }),
  ).toBeVisible();
  await row(page, 'Cove').click();
  await expect(page).toHaveURL(/\?category=101$/);
  await page.goBack();
  await expect(page).toHaveURL(/\?category=123$/);
  await expect(
    page.getByRole('heading', { name: 'suncityarena', level: 2 }),
  ).toBeVisible();
  await page.goBack();
  await expect(
    page.getByRole('heading', { name: 'Select a category' }),
  ).toBeVisible();
  await page.goForward();
  await expect(
    page.getByRole('heading', { name: 'suncityarena', level: 2 }),
  ).toBeVisible();

  await page.goto('/demo/categories?category=99999');
  await expect(
    page.getByRole('heading', { name: 'Select a category' }),
  ).toBeVisible();
  await expect(page.getByText(/Could not/)).toHaveCount(0);
  expect(await noHorizontalScroll(page)).toBe(true);
  expect(errors).toEqual([]);
});

test('flat workspace: plain subtitle, Cursor dots, no literal-pattern card', async ({
  page,
}) => {
  const { errors } = await boot(page, {
    list: FLAT(),
    workspace: 'TMGR.DEV',
    size: { width: 1440, height: 900 },
  });
  await page.goto('/demo/categories');
  await expect(page.locator('[data-page-header] p')).toHaveText('6 categories');
  await expect(page.getByRole('option')).toHaveCount(6);
  await expect(page.getByTitle('Cursor connected')).toHaveCount(2);
  await expect(row(page, 'TC')).toContainText('TC-#: · 12/h · 3h');
  await expect(page.getByText(/name new tasks literally/)).toHaveCount(0);
  await row(page, 'TM').click();
  await expect(pane(page)).toContainText('TMGR.DEV · top level');
  await expect(
    page.getByText('Preview', { exact: true }).locator('..'),
  ).toContainText('TM-#: Task title');
  expect(errors).toEqual([]);
});

test('literal pattern: the fix button rewrites the field without saving; chips insert at the cursor', async ({
  page,
}) => {
  const { state, errors } = await boot(page);
  await page.goto('/demo/categories?category=123');
  const field = patternField(page);
  await expect(field).toHaveValue('DD.MM.YYYY');
  await expect(field).toHaveAttribute('aria-invalid', 'true');
  await expect(
    page.getByText(
      /No placeholders\. Every new task will literally start with “DD\.MM\.YYYY”/,
    ),
  ).toBeVisible();
  await expect(
    page.getByText('Preview', { exact: true }).locator('..'),
  ).toContainText('DD.MM.YYYY: Task title');
  await expect(page.getByRole('button', { name: 'Discard' })).toBeDisabled();

  await page.getByRole('button', { name: 'Use {dts#dd.mm.yyyy#dte}' }).click();
  await expect(field).toHaveValue('{dts#dd.mm.yyyy#dte}');
  await expect(field).not.toHaveAttribute('aria-invalid', 'true');
  await expect(
    page.getByText('Preview', { exact: true }).locator('..'),
  ).toContainText('12.10.2026: Task title');
  await expect(page.getByRole('button', { name: 'Discard' })).toBeEnabled();
  await page.waitForTimeout(300);
  expect(writes(state, /^PUT /)).toEqual([]);

  await field.evaluate((el) => el.setSelectionRange(0, 0));
  await page.getByRole('button', { name: '+ category number' }).click();
  await expect(field).toHaveValue('{index#category}{dts#dd.mm.yyyy#dte}');

  await page.getByRole('button', { name: 'Discard' }).click();
  await expect(field).toHaveValue('DD.MM.YYYY');
  expect(writes(state, /^PUT /)).toEqual([]);
  expect(errors).toEqual([]);
});

test('typing words before a date token keeps the preview working', async ({
  page,
}) => {
  const { errors } = await boot(page, { list: FLAT() });
  await page.goto('/demo/categories?category=127');
  await patternField(page).fill('Sprint {dts#dd#dte}');
  await expect(
    page.getByText('Preview', { exact: true }).locator('..'),
  ).toContainText('Sprint 12: Task title');
  await expect(patternField(page)).toBeVisible();
  expect(errors).toEqual([]);
});

test('a stored pattern with words before a date token does not break the list', async ({
  page,
}) => {
  await boot(page, {
    list: [
      ...FLAT(),
      cat(300, 'Reports', 'REP', null, { pat: 'Report {dts#dd.mm#dte}' }),
    ],
  });
  await page.goto('/demo/categories');
  await expect(row(page, 'Reports')).toContainText('Report 12.10');
});

test('Fix all rewrites every literal pattern; a partial failure reports the count', async ({
  page,
}) => {
  const { state, errors } = await boot(page);
  state.fail['PUT project_categories/94/settings'] = {
    status: 400,
    json: { message: 'Nope' },
  };
  await page.goto('/demo/categories');
  await page.getByRole('button', { name: 'Fix all 7' }).click();
  await expect(
    page.getByText('Fixed 6 of 7 title patterns', { exact: true }),
  ).toBeVisible();
  const puts = writes(state, /^PUT project_categories\/\d+\/settings$/);
  expect(puts.map((c) => Number(c.path.split('/')[1])).sort()).toEqual(
    [...LITERAL_IDS].sort(),
  );
  const suncity = puts.find(
    (c) => c.path === 'project_categories/123/settings',
  ).body;
  expect(suncity).toEqual([
    { id: 1, value: '{dts#dd.mm.yyyy#dte}' },
    { id: 2, value: '700' },
    { id: 3, value: '28800' },
    { id: 4, value: '3600' },
  ]);
  await expect(
    page.getByText('1 category names new tasks literally'),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Fix all 1' })).toBeVisible();
  expect(errors).toEqual([]);
});

test('create at the top level and a subcategory: exact buttons, draft row, copy default, select the new one', async ({
  page,
}) => {
  const { state, errors } = await boot(page);
  await page.goto('/demo/categories');
  await page.getByRole('button', { name: 'New category' }).click();
  await expect(page).toHaveURL(/\?create=top$/);
  await expect(
    page.getByRole('heading', { name: 'New category', level: 2 }),
  ).toBeVisible();
  await expect(pane(page)).toContainText('Personal · top level');
  await expect(page.getByPlaceholder('Name')).toBeFocused();
  await expect(
    page.getByRole('button', { name: 'Cancel', exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Create', exact: true }),
  ).toBeVisible();
  await expect(page.getByText('Copy settings from')).toHaveCount(0);
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page).toHaveURL(/\/demo\/categories$/);
  await expect(
    page.getByRole('heading', { name: 'Select a category' }),
  ).toBeVisible();

  await row(page, 'Freelance').click();
  await page.getByRole('button', { name: '+ Subcategory' }).click();
  await expect(page).toHaveURL(/\?create=27$/);
  await expect(
    page.getByRole('option', { name: 'New subcategory' }),
  ).toBeVisible();
  await expect(pane(page)).toContainText('Work / Freelance /');
  await expect(page.getByPlaceholder('Name')).toBeFocused();
  await expect(page.getByRole('combobox', { name: 'Parent' })).toHaveText(
    'Freelance',
  );
  await expect(
    page.getByRole('combobox', { name: 'Copy settings from' }),
  ).toHaveText('FCPP');
  await expect(patternField(page)).toHaveValue(DATE_DAY);

  await page.getByPlaceholder('Name').fill('Newco');
  await page.getByPlaceholder('e.g., PROJ, FEAT, BUG').fill('new co');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page).toHaveURL(/\?category=900$/);
  await expect(
    page.getByRole('heading', { name: 'Newco', level: 2 }),
  ).toBeVisible();
  await expect(row(page, 'Newco')).toHaveAttribute('aria-selected', 'true');
  await expect(pane(page)).toContainText('Work / Freelance /');
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

test('a dirty form asks before switching selection, starting a create or going back', async ({
  page,
}) => {
  const { errors } = await boot(page);
  await page.goto('/demo/categories');
  await row(page, 'Cove').click();
  await page.getByPlaceholder('Name').fill('Cove 2');
  const dialog = page.getByRole('alertdialog');

  await row(page, 'FCPP').click();
  await expect(dialog).toContainText('Discard unsaved changes?');
  await dialog.getByRole('button', { name: 'Keep editing' }).click();
  await expect(dialog).toBeHidden();
  await expect(page).toHaveURL(/\?category=101$/);
  await expect(page.getByPlaceholder('Name')).toHaveValue('Cove 2');

  await page.getByRole('button', { name: 'New category' }).click();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Keep editing' }).click();
  await page.goBack();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Keep editing' }).click();
  await expect(page).toHaveURL(/\?category=101$/);

  await row(page, 'FCPP').click();
  await dialog.getByRole('button', { name: 'Discard' }).click();
  await expect(page).toHaveURL(/\?category=151$/);
  await expect(page.getByPlaceholder('Name')).toHaveValue('FCPP');
  expect(errors).toEqual([]);
});

test('save sends the category and its full settings, then the list shows the new values', async ({
  page,
}) => {
  const { state, errors } = await boot(page);
  await page.goto('/demo/categories?category=101');
  await page.getByPlaceholder('Name').fill('Cove Ltd');
  await page.getByPlaceholder('—').fill('800');
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
  await expect(page.getByRole('button', { name: 'Discard' })).toBeDisabled();
  await expect(row(page, 'Cove Ltd')).toContainText(
    '12.10.2026 Mon · 800/h · 8h',
  );
  await expect(
    page.getByRole('heading', { name: 'Cove Ltd', level: 2 }),
  ).toBeVisible();
  expect(errors).toEqual([]);
});

test('a 422 shows inline errors and keeps the form dirty; delete returns to nothing selected', async ({
  page,
}) => {
  const { state, errors } = await boot(page);
  state.fail['PUT project_categories/101'] = {
    status: 422,
    json: {
      message: 'The given data was invalid.',
      errors: { title: ['The title has already been taken.'] },
    },
  };
  await page.goto('/demo/categories?category=101');
  await page.getByPlaceholder('Name').fill('FCPP');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(
    page.getByText('The title has already been taken.'),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Discard' })).toBeEnabled();
  await expect(page.getByPlaceholder('Name')).toHaveValue('FCPP');
  expect(writes(state, /settings$/)).toEqual([]);

  await page.getByRole('button', { name: 'Discard' }).click();
  await page.getByRole('button', { name: 'Delete…' }).click();
  await page
    .getByRole('alertdialog')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(page).toHaveURL(/\/demo\/categories$/);
  await expect(
    page.getByRole('heading', { name: 'Select a category' }),
  ).toBeVisible();
  await expect(row(page, 'Cove')).toHaveCount(0);
  expect(writes(state, /^DELETE /).map((c) => c.path)).toEqual([
    'project_categories/101',
  ]);
  expect(errors).toEqual([]);
});

test('empty workspace: one centred call to action, no two-pane card', async ({
  page,
}) => {
  const { errors } = await boot(page, { list: [] });
  await page.goto('/demo/categories');
  await expect(
    page.getByRole('heading', { name: 'No categories yet' }),
  ).toBeVisible();
  await expect(page.locator('[data-tour="categories.create"]')).toHaveCount(1);
  await expect(page.locator('[data-tour="categories.create"]')).toHaveText(
    'New category',
  );
  await expect(page.getByRole('listbox')).toHaveCount(0);
  await page.locator('[data-tour="categories.create"]').click();
  await expect(page).toHaveURL(/\?create=top$/);
  expect(errors).toEqual([]);
});

test('old category URLs land on the pane in the right state', async ({
  page,
}) => {
  await boot(page);
  for (const [from, to] of [
    ['/demo/categories/create', /\/demo\/categories\?create=top$/],
    ['/demo/categories/101', /\/demo\/categories\?category=101$/],
    ['/demo/categories/27/create', /\/demo\/categories\?create=27$/],
    ['/demo/categories/status/active', /\/demo\/categories$/],
  ]) {
    await page.goto(from);
    await expect(page, from).toHaveURL(to);
  }
  await expect(
    page.getByRole('heading', { name: 'Select a category' }),
  ).toBeVisible();
});

test('phone: list of group cards, full-screen settings, back to the list', async ({
  page,
}) => {
  const { errors } = await boot(page, { size: { width: 390, height: 844 } });
  await page.goto('/demo/categories');
  await expect(page.locator('[data-tour="categories.create"]')).toBeVisible();
  await expect(page.getByRole('listbox').getByRole('group')).toHaveCount(3);
  expect(
    Math.round((await row(page, 'Cove').boundingBox()).height),
  ).toBeGreaterThanOrEqual(52);
  expect(await noHorizontalScroll(page)).toBe(true);
  await row(page, 'Cove').click();
  await expect(page.getByRole('listbox')).toBeHidden();
  await expect(page.getByRole('link', { name: 'Tasks →' })).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Save', exact: true }),
  ).toBeVisible();
  expect(await noHorizontalScroll(page)).toBe(true);
  await page.getByRole('button', { name: 'Back to categories' }).click();
  await expect(page).toHaveURL(/\/demo\/categories$/);
  await expect(row(page, 'Cove')).toBeVisible();
  expect(errors).toEqual([]);
});

const SHOTS = process.env.CATEGORIES_SHOTS;
const STATES = [
  ['None', '/demo/categories', NESTED, 'Personal', 'Phone'],
  ['NoneClean', '/demo/categories', FLAT, 'TMGR.DEV'],
  ['Edit', '/demo/categories?category=101', NESTED, 'Personal', 'PhoneEdit'],
  ['Warn', '/demo/categories?category=123', NESTED, 'Personal'],
  ['Create', '/demo/categories?create=27', NESTED, 'Personal'],
  ['Flat', '/demo/categories?category=127', FLAT, 'TMGR.DEV'],
  ['Empty', '/demo/categories', () => [], 'Personal'],
];

for (const [state, url, list, workspace, phoneName] of STATES) {
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
            .locator('[data-page-header] h1:visible, section h2:visible')
            .first(),
        ).toBeVisible();
        await page.waitForLoadState('networkidle');
        await page.waitForTimeout(500);
        const base = state === 'Empty' ? 'R3-Empty' : `R3A-${state}`;
        const name =
          width === 1440
            ? base
            : phoneName
            ? `R3A-${phoneName}`
            : `${base}-390`;
        await page.screenshot({ path: `${SHOTS}/${name}-${scheme}.png` });
      });
    }
  }
}
