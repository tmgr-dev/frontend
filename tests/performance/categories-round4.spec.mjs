import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const CATALOGUE = [
  {
    id: 1,
    name: 'The date&time pattern',
    key: 'task_name_pattern_date&time',
    component_type: 'text',
    description: '',
    custom_value_available: true,
    variable_type: 'string',
    rules: 'sometimes|string',
    default_values: [],
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
];

const cat = (id, title, code, parent, values = {}) => ({
  id,
  title,
  code,
  slug: code.toLowerCase(),
  project_category_id: parent,
  workspace_id: 1,
  user_id: 1,
  cursor_connected: false,
  deleted_at: null,
  children_count: 0,
  tasks_count: 0,
  settings: CATALOGUE.filter((d) => values[d.id] != null).map((d) => ({
    id: d.id,
    name: d.name,
    key: d.key,
    description: '',
    value: String(values[d.id]),
  })),
});

const TREE = () => [
  cat(24, 'Work', 'WORK', null),
  cat(101, 'Cove', 'COVE', 24, { 1: 'CV-{index#category}', 2: 700, 3: 5430 }),
  cat(27, 'Freelance', 'FREELANCE', 24),
  cat(94, 'If-so.com', 'IFSOCOM', 27),
  cat(47, 'Common projects', 'COMMONPROJECTS', null),
];

const boot = async (page, { list = TREE(), size } = {}) => {
  await page.setViewportSize(size ?? { width: 1440, height: 900 });
  await page.clock.setFixedTime(new Date('2026-10-12T10:00:00'));
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await mockApp(page);
  await page.route('**/api/workspaces', (route) =>
    route.fulfill({
      json: {
        data: [
          { id: 1, name: 'Personal', code: 'demo', user_id: 1, is_default: 1 },
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
            ['board', 'categories'].map((key) => [
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
  const state = { list, calls: [], nextId: 900, listFail: 0 };
  await page.route('**/api/project_categories**', async (route) => {
    const request = route.request();
    const method = request.method();
    const path = new URL(request.url()).pathname.replace(/^.*\/api\//, '');
    const body =
      method === 'GET' || method === 'DELETE' ? null : request.postDataJSON();
    if (method !== 'GET') state.calls.push({ method, path, body });
    const find = (id) => state.list.find((c) => c.id === Number(id));
    if (method === 'GET' && path === 'project_categories') {
      if (state.listFail)
        return route.fulfill({
          status: state.listFail,
          json: { message: 'Not allowed' },
        });
      return route.fulfill({ json: { data: state.list } });
    }
    if (method === 'GET' && path === 'project_categories/settings')
      return route.fulfill({ json: { data: CATALOGUE } });
    let m = path.match(/^project_categories\/children\/(\d+)$/);
    if (m && method === 'GET') {
      const rows = state.list.filter(
        (c) => c.project_category_id === Number(m[1]),
      );
      return route.fulfill({
        json: {
          data: rows,
          meta: {
            current_page: 1,
            last_page: 1,
            per_page: 15,
            total: rows.length,
            from: 1,
            to: rows.length,
          },
        },
      });
    }
    m = path.match(/^project_categories\/(\d+)\/with\/parents$/);
    if (m) return route.fulfill({ json: { data: find(m[1]) } });
    if (method === 'POST' && path === 'project_categories') {
      const created = cat(
        state.nextId++,
        body.title,
        body.code || 'NEW',
        body.project_category_id ?? null,
      );
      state.list = [...state.list, created];
      return route.fulfill({ status: 201, json: { data: created } });
    }
    m = path.match(/^project_categories\/(\d+)\/restore$/);
    if (m) {
      find(m[1]).deleted_at = null;
      return route.fulfill({ json: { data: { ...find(m[1]) } } });
    }
    m = path.match(/^project_categories\/(\d+)\/settings$/);
    if (m && method === 'PUT') {
      find(m[1]).settings = CATALOGUE.filter((d) =>
        body.some((s) => s.id === d.id && s.value !== ''),
      ).map((d) => ({
        id: d.id,
        name: d.name,
        key: d.key,
        description: '',
        value: body.find((s) => s.id === d.id).value,
      }));
      return route.fulfill({ json: { data: { ...find(m[1]) } } });
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
      find(m[1]).deleted_at = '2026-10-12T10:00:00Z';
      return route.fulfill({ json: { data: { ...find(m[1]) } } });
    }
    return route.fulfill({ json: { data: [] } });
  });
  return { state, errors };
};

const escapeRe = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const sidebar = (page) => page.getByTestId('categories-sidebar-section');
const treeRow = (page, name) =>
  sidebar(page).getByRole('link', {
    name: new RegExp(`^${escapeRe(name)}(\\s|$)`),
  });
const header = (page) => page.locator('[data-page-header]');
const tabs = (page) => page.getByRole('navigation', { name: 'Category' });
const bar = (page) => page.getByText(/^\d+ unsaved changes?$/);
const nameField = (page) =>
  page.getByRole('textbox', { name: 'Name', exact: true });
const missing = (page) => page.getByTestId('category-missing');
const writes = (state, re) =>
  state.calls.filter((c) => re.test(`${c.method} ${c.path}`));

test.describe.configure({ timeout: 90_000 });

test('sidebar: the section replaces the Categories link and Recent categories; filter matches a code only', async ({
  page,
}) => {
  const { errors } = await boot(page);
  await page.goto('/demo/categories/101/children');
  await expect(treeRow(page, 'Cove')).toHaveAttribute('aria-current', 'page');
  await expect(page.getByText('Recent categories')).toHaveCount(0);
  await expect(
    page.getByRole('link', { name: 'Categories', exact: true }),
  ).toHaveCount(0);
  await sidebar(page)
    .getByRole('button', { name: 'Filter categories' })
    .click();
  await sidebar(page)
    .getByRole('searchbox', { name: 'Filter categories' })
    .fill('ifsocom');
  await expect(sidebar(page).getByRole('link')).toHaveCount(3);
  for (const name of ['Work', 'Freelance', 'If-so.com'])
    await expect(treeRow(page, name)).toBeVisible();
  await sidebar(page)
    .getByRole('searchbox', { name: 'Filter categories' })
    .fill('zzz');
  await expect(sidebar(page)).toContainText('No categories match “zzz”');
  expect(errors).toEqual([]);
});

test('unknown parents, ids and tabs never render a blank page', async ({
  page,
}) => {
  const { errors } = await boot(page);
  for (const url of [
    '/demo/categories/99999/children',
    '/demo/categories/abc/children',
    '/demo/categories/99999/create',
    '/demo/categories?create=99999',
  ]) {
    await page.goto(url);
    await expect(missing(page), url).toContainText('Category not found');
  }
  await missing(page).getByRole('button', { name: 'Open categories' }).click();
  await expect(page).toHaveURL(/\/demo\/categories\/47\/children$/);

  await page.goto('/demo/categories/101/children?tab=bogus');
  await expect(tabs(page).getByRole('link', { name: 'Tasks' })).toHaveAttribute(
    'aria-current',
    'page',
  );
  expect(errors).toEqual([]);
});

test('the last opened category is kept per workspace; a stale one falls back to the first root', async ({
  page,
}) => {
  await boot(page);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('seeded')) return;
    sessionStorage.setItem('seeded', '1');
    localStorage.setItem('tmgr:lastCategory:other', '101');
    localStorage.setItem('tmgr:lastCategory:demo', '99999');
  });
  await page.goto('/demo/categories');
  await expect(page).toHaveURL(/\/demo\/categories\/47\/children$/);
  await page.goto('/demo/categories/27/children?tab=settings');
  await expect(nameField(page)).toHaveValue('Freelance');
  await page.goto('/demo/categories');
  await expect(page).toHaveURL(/\/demo\/categories\/27\/children$/);
  expect(
    await page.evaluate(() => localStorage.getItem('tmgr:lastCategory:demo')),
  ).toBe('27');
});

test('a failed category list offers Retry in the page and the sidebar; 403 shows Access Denied', async ({
  page,
}) => {
  const { state } = await boot(page);
  state.listFail = 400;
  await page.goto('/demo/categories/101/children');
  await expect(sidebar(page)).toContainText('Failed to load categories');
  await expect(page.getByText('Not allowed')).toBeVisible();
  state.listFail = 0;
  await sidebar(page).getByRole('button', { name: 'Retry' }).click();
  await expect(header(page).locator('h1')).toHaveText('Cove');
  await expect(treeRow(page, 'Cove')).toHaveAttribute('aria-current', 'page');

  state.listFail = 403;
  await page.reload();
  await expect(page.getByText('Access Denied')).toBeVisible();
});

test('save writes the category before its settings and a double click saves once; double Create posts once', async ({
  page,
}) => {
  const { state, errors } = await boot(page);
  await page.goto('/demo/categories/101/children?tab=settings');
  await nameField(page).fill('Cove Ltd');
  await page.getByRole('button', { name: 'Save', exact: true }).dblclick();
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  await page.waitForTimeout(500);
  expect(
    writes(state, /^PUT project_categories\/101/).map((c) => c.path),
  ).toEqual(['project_categories/101', 'project_categories/101/settings']);

  await page.goto('/demo/categories/create');
  await nameField(page).fill('Solo');
  await page.getByRole('button', { name: 'Create', exact: true }).dblclick();
  await expect(page).toHaveURL(/\/categories\/900\/children\?tab=settings$/);
  await page.waitForTimeout(500);
  expect(writes(state, /^POST project_categories$/)).toHaveLength(1);
  expect(errors).toEqual([]);
});

test('a stored duration that is not whole minutes survives an unrelated save; a cleared one is sent empty', async ({
  page,
}) => {
  const { state } = await boot(page);
  await page.goto('/demo/categories/101/children?tab=settings');
  const estimate = page.getByRole('textbox', { name: 'Default estimate' });
  await expect(estimate).toHaveValue('1h 30m');
  await expect(bar(page)).toHaveCount(0);
  await page.getByRole('textbox', { name: 'Rate per hour' }).fill('800');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(bar(page)).toHaveCount(0);
  expect(
    writes(state, /^PUT project_categories\/101\/settings$/)[0].body,
  ).toContainEqual({ id: 3, value: '5430' });

  await estimate.fill('');
  await expect(bar(page)).toHaveText('1 unsaved change');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(bar(page)).toHaveCount(0);
  expect(
    writes(state, /^PUT project_categories\/101\/settings$/)[1].body,
  ).toContainEqual({ id: 3, value: '' });
  await expect(estimate).toHaveValue('');
});

test('Cancel leaves an untouched create without asking, a dirty one asks', async ({
  page,
}) => {
  await boot(page);
  await page.goto('/demo/categories/101/create');
  await expect(nameField(page)).toBeFocused();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page).toHaveURL(/\/demo\/categories\/101\/children$/);
  await expect(page.getByRole('alertdialog')).toHaveCount(0);

  await page.goto('/demo/categories/create');
  await nameField(page).fill('Draft');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.getByRole('alertdialog')).toContainText(
    'Discard unsaved changes?',
  );
});

test('Subcategories tab: Edit opens the child Settings tab; Delete and Restore update the tree', async ({
  page,
}) => {
  const { state, errors } = await boot(page);
  await page.goto('/demo/categories/24/children?tab=subcategories');
  await expect(page.getByRole('heading', { name: 'Cove' })).toBeVisible();
  await sidebar(page).getByRole('button', { name: 'Expand' }).first().click();
  await expect(page.getByRole('heading', { name: 'Freelance' })).toBeVisible();
  const menuOf = (name) =>
    page
      .locator('div.relative.cursor-pointer')
      .filter({ has: page.getByRole('heading', { name }) })
      .locator('button[aria-haspopup="menu"]');
  await expect(treeRow(page, 'Cove')).toBeVisible();

  await menuOf('Cove').click();
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  await expect(page.getByText('(deleted, but can be restored)')).toBeVisible();
  await expect(treeRow(page, 'Cove')).toHaveCount(0);
  await menuOf('Cove').click();
  await page.getByRole('menuitem', { name: 'Restore' }).click();
  await expect(treeRow(page, 'Cove')).toBeVisible();
  expect([...new Set(state.calls.map((c) => `${c.method} ${c.path}`))]).toEqual(
    ['DELETE project_categories/101', 'POST project_categories/101/restore'],
  );

  await menuOf('Freelance').click();
  await page.getByRole('menuitem', { name: 'Edit' }).click();
  await expect(page).toHaveURL(
    /\/demo\/categories\/27\/children\?tab=settings$/,
  );
  await expect(nameField(page)).toHaveValue('Freelance');
  expect(errors).toEqual([]);
});

test('deleting the only category shows the empty state', async ({ page }) => {
  const { errors } = await boot(page, {
    list: [cat(5, 'Only', 'ONLY', null)],
  });
  await page.goto('/demo/categories/5/children?tab=settings');
  await page.getByRole('button', { name: 'Delete…' }).click();
  await page
    .getByRole('alertdialog')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(page).toHaveURL(/\/demo\/categories$/);
  await expect(
    page.getByRole('heading', { name: 'No categories yet' }),
  ).toBeVisible();
  await expect(sidebar(page).getByRole('link')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('settings is flat: no inner scroll box or bordered card around the form; integrations expand inline', async ({
  page,
}) => {
  await boot(page);
  await page.goto('/demo/categories/101/children?tab=settings');
  await expect(nameField(page)).toHaveValue('Cove');
  const frames = await nameField(page).evaluate((input) => {
    const out = [];
    for (let el = input.parentElement; el; el = el.parentElement) {
      if (el.querySelector('[data-page-header]')) break;
      const s = getComputedStyle(el);
      const bordered = ['Top', 'Right', 'Bottom', 'Left'].every(
        (side) => parseFloat(s[`border${side}Width`]) > 0,
      );
      if (bordered || /auto|scroll/.test(s.overflowY))
        out.push(`${el.tagName}.${el.className}`);
    }
    return out;
  });
  expect(frames).toEqual([]);

  await page.getByRole('button', { name: 'Connect', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Close', exact: true }).first(),
  ).toHaveAttribute('aria-expanded', 'true');
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('phone: the save bar and the create bar stay pinned in the viewport', async ({
  page,
}) => {
  await boot(page, { size: { width: 390, height: 844 } });
  await page.goto('/demo/categories/101/children?tab=settings');
  await nameField(page).fill('Cove 2');
  await expect(bar(page)).toBeVisible();
  await expect
    .soft(page.getByRole('button', { name: 'Save', exact: true }))
    .toBeInViewport();
  await page.getByRole('button', { name: 'Discard', exact: true }).click();

  await page.goto('/demo/categories/create');
  await expect
    .soft(page.getByRole('button', { name: 'Create', exact: true }))
    .toBeInViewport();
});

test('phone: + in the drawer opens the create form with Name focused', async ({
  page,
}) => {
  await boot(page, { size: { width: 390, height: 844 } });
  await page.goto('/demo/categories/101/children');
  await expect(header(page).locator('h1')).toHaveText('Cove');
  await page.waitForLoadState('networkidle');
  await page.locator('[data-tour="nav.menu-button"]').click();
  await sidebar(page).getByRole('button', { name: 'New category' }).click();
  await expect(page).toHaveURL(/\/demo\/categories\/create$/);
  await page.waitForTimeout(800);
  await expect(nameField(page)).toBeFocused();
});

test('phone: tapping a tree row or + closes the drawer', async ({ page }) => {
  await boot(page, { size: { width: 390, height: 844 } });
  await page.goto('/demo/categories/47/children');
  await expect(header(page).locator('h1')).toHaveText('Common projects');
  await page.waitForLoadState('networkidle');
  const drawer = page.locator('[data-sidebar="sidebar"][data-mobile="true"]');
  await page.locator('[data-tour="nav.menu-button"]').click();
  await expect(drawer).toBeVisible();
  await treeRow(page, 'Work').click();
  await expect(page).toHaveURL(/\/demo\/categories\/24\/children$/);
  await expect(drawer).toBeHidden();

  await page.locator('[data-tour="nav.menu-button"]').click();
  await sidebar(page).getByRole('button', { name: 'New category' }).click();
  await expect(page).toHaveURL(/\/demo\/categories\/create$/);
  await expect(drawer).toBeHidden();
  const nav = await page.evaluate(
    () =>
      document.documentElement.scrollWidth <=
      document.documentElement.clientWidth,
  );
  expect(nav).toBe(true);
});
