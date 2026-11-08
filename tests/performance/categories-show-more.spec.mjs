import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const cat = (id, title, parent = null) => ({
  id,
  title,
  code: `C${id}`,
  slug: `c${id}`,
  project_category_id: parent,
  workspace_id: 1,
  user_id: 1,
  cursor_connected: false,
  deleted_at: null,
  children_count: 0,
  tasks_count: 0,
  settings: [],
});

const NAMES = ['Alpha', 'Bravo', 'Charlie', 'Delta', 'Echo', 'Foxtrot', 'Golf'];

const boot = async (page, count, size = { width: 1440, height: 900 }) => {
  await page.setViewportSize(size);
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
  const list = NAMES.slice(0, count).map((n, i) => cat(i + 1, n));
  await page.route('**/api/project_categories**', (route) => {
    const path = new URL(route.request().url()).pathname.replace(
      /^.*\/api\//,
      '',
    );
    if (path === 'project_categories')
      return route.fulfill({ json: { data: list } });
    return route.fulfill({ json: { data: [] } });
  });
  return errors;
};

const sidebar = (page) => page.getByTestId('categories-sidebar-section');
const links = (page) => sidebar(page).getByRole('link');
const toggle = (page) => sidebar(page).getByTestId('categories-show-toggle');

test.describe.configure({ timeout: 90_000 });

test('up to 5 categories: no toggle', async ({ page }) => {
  const errors = await boot(page, 5);
  await page.goto('/demo/categories/1/children');
  await expect(links(page)).toHaveCount(5);
  await expect(toggle(page)).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('more than 5: Show more expands, Show less collapses', async ({
  page,
}) => {
  const errors = await boot(page, 7);
  await page.goto('/demo/categories/1/children');
  await expect(links(page)).toHaveCount(5);
  await expect(toggle(page)).toHaveText('Show 2 more');
  await sidebar(page).screenshot({
    path: process.env.SHOT_DIR
      ? `${process.env.SHOT_DIR}/collapsed.png`
      : undefined,
  });
  await toggle(page).click();
  await expect(links(page)).toHaveCount(7);
  await expect(toggle(page)).toHaveText('Show less');
  await sidebar(page).screenshot({
    path: process.env.SHOT_DIR
      ? `${process.env.SHOT_DIR}/expanded.png`
      : undefined,
  });
  await toggle(page).click();
  await expect(links(page)).toHaveCount(5);
  await expect(toggle(page)).toHaveText('Show 2 more');
  expect(errors).toEqual([]);
});

test('active category past the first 5 starts expanded and can collapse', async ({
  page,
}) => {
  const errors = await boot(page, 7);
  await page.goto('/demo/categories/7/children');
  await expect(links(page)).toHaveCount(7);
  await expect(
    sidebar(page).getByRole('link', { name: 'Golf' }),
  ).toHaveAttribute('aria-current', 'page');
  await toggle(page).click();
  await expect(links(page)).toHaveCount(5);
  expect(errors).toEqual([]);
});

test('filter shows matches without a toggle and the empty message', async ({
  page,
}) => {
  const errors = await boot(page, 7);
  await page.goto('/demo/categories/1/children');
  await sidebar(page)
    .getByRole('button', { name: 'Filter categories' })
    .click();
  const input = sidebar(page).getByRole('searchbox', {
    name: 'Filter categories',
  });
  await input.fill('golf');
  await expect(links(page)).toHaveCount(1);
  await expect(toggle(page)).toHaveCount(0);
  await input.fill('zzz');
  await expect(sidebar(page)).toContainText('No categories match');
  expect(errors).toEqual([]);
});

test('narrow viewport, dark theme', async ({ page }) => {
  const errors = await boot(page, 7, { width: 390, height: 800 });
  await page.addInitScript(() => localStorage.setItem('colorScheme', 'dark'));
  await page.goto('/demo/categories/1/children');
  await page
    .getByRole('button', { name: /toggle sidebar|menu/i })
    .first()
    .click()
    .catch(() => {});
  await expect(toggle(page)).toHaveText('Show 2 more');
  await page.screenshot({
    path: process.env.SHOT_DIR
      ? `${process.env.SHOT_DIR}/narrow-dark.png`
      : undefined,
  });
  expect(errors).toEqual([]);
});
