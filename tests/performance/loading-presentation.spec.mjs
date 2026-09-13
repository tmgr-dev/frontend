import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test('shared loading boundary reserves shape, preserves focused content and supports keyboard retry', async ({
  page,
}) => {
  await mockApp(page);
  await page.goto('/');
  await expect(page.locator('[data-task-id="1"]')).toBeVisible();
  await page.evaluate(async () => {
    const { createApp, h, reactive, ref } = await import(
      '/node_modules/.vite/deps/vue.js'
    );
    const { default: AsyncContent } = await import(
      '/src/components/async/AsyncContent.vue'
    );
    const host = document.createElement('div');
    host.id = 'loading-fixture';
    document.body.append(host);
    window.__loadingState = reactive({
      pending: true,
      loaded: false,
      error: null,
    });
    const DraftInput = {
      setup() {
        const draft = ref('Draft');
        return () =>
          h('input', {
            'aria-label': 'Persistent draft',
            value: draft.value,
            onInput: (e) => (draft.value = e.target.value),
          });
      },
    };
    createApp({
      setup: () => () =>
        h(
          AsyncContent,
          {
            ...window.__loadingState,
            retry: () => {
              window.__loadingState.error = null;
              window.__loadingState.pending = true;
            },
          },
          {
            skeleton: () =>
              h('div', {
                style: 'height:200px',
                'data-testid': 'reserved-shape',
              }),
            default: () => h(DraftInput),
          },
        ),
    }).mount(host);
  });
  const region = page.locator('#loading-fixture');
  await expect(region.getByTestId('reserved-shape')).toBeVisible();
  await expect(region.locator('[aria-busy="true"]')).toHaveCount(1);
  await page.evaluate(() => {
    window.__loadingState.pending = false;
    window.__loadingState.loaded = true;
  });
  const input = region.getByRole('textbox');
  await input.fill('Unsaved draft');
  await input.focus();
  await page.evaluate(() => {
    window.__loadingState.pending = true;
  });
  await expect(input).toBeFocused();
  await expect(input).toHaveValue('Unsaved draft');
  await expect(region.getByTestId('reserved-shape')).toHaveCount(0);
  await page.evaluate(() => {
    window.__loadingState.pending = false;
    window.__loadingState.error = 'Offline';
  });
  await expect(region.getByRole('alert')).toContainText('Offline');
  await expect(input).toHaveValue('Unsaved draft');
  await region.getByRole('button', { name: 'Try again' }).focus();
  await page.keyboard.press('Enter');
  await expect(region.locator('[aria-busy="true"]')).toHaveCount(1);
  await expect(input).toHaveValue('Unsaved draft');
});

test('notification next-page failure preserves old rows and retries the failed page', async ({
  page,
}) => {
  await mockApp(page);
  let failed = true;
  const pages = [];
  await page.route('**/api/notifications?*', async (route) => {
    const pageNumber = Number(
      new URL(route.request().url()).searchParams.get('page'),
    );
    pages.push(pageNumber);
    if (pageNumber === 2 && failed) {
      await route.fulfill({
        status: 400,
        json: { message: 'Page unavailable' },
      });
      return;
    }
    await route.fulfill({
      json: {
        data: [
          {
            id: pageNumber,
            title: `Notification page ${pageNumber}`,
            message: 'Fixture notice',
            type: 'task_created',
            created_at: new Date().toISOString(),
            read_at: null,
          },
        ],
        current_page: pageNumber,
        last_page: 2,
        per_page: 20,
        total: 2,
      },
    });
  });
  await page.goto('/demo/notifications');
  await expect(
    page.getByText('Notification page 1', { exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Load more' }).click();
  await expect(page.getByRole('alert')).toContainText('Page unavailable');
  await expect(
    page.getByText('Notification page 1', { exact: true }),
  ).toBeVisible();
  failed = false;
  await page.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(
    page.getByText('Notification page 2', { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText('Notification page 1', { exact: true }),
  ).toBeVisible();
  // The notification bell independently reads page one.
  expect(pages.slice(-2)).toEqual([2, 2]);
});

test('category task read errors end loading and retry without replacing category content', async ({
  page,
}) => {
  await mockApp(page);
  let fail = true;
  await page.route('**/api/workspaces/*/feature-toggles', (route) =>
    route.fulfill({ json: { data: { categories: { enabled: true } } } }),
  );
  await page.route('**/api/project_categories/children/1?*', (route) =>
    route.fulfill({
      json: {
        data: [],
        meta: { current_page: 1, per_page: 10, total: 0, last_page: 1 },
      },
    }),
  );
  await page.route('**/api/project_categories/1/with/parents', (route) =>
    route.fulfill({
      json: { data: { id: 1, title: 'Visible parent', parent: null } },
    }),
  );
  await page.route('**/api/tasks/?*', (route) =>
    route.fulfill(
      fail
        ? { status: 400, json: { message: 'Category tasks unavailable' } }
        : {
            json: {
              data: [],
              meta: { current_page: 1, per_page: 10, total: 0, last_page: 1 },
            },
          },
    ),
  );
  await page.goto('/demo/categories/1/children');
  await expect(page.getByRole('alert')).toContainText(
    'Category tasks unavailable',
  );
  await expect(page.getByTitle('Edit Visible parent category')).toBeVisible();
  await expect(
    page.getByText('Create your first task in this category'),
  ).toHaveCount(0);
  fail = false;
  await page.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(
    page.getByText('Create your first task in this category'),
  ).toBeVisible();
  await expect(page.getByTitle('Edit Visible parent category')).toBeVisible();
});
