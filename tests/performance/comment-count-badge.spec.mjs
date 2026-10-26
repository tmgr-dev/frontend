import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

// TM-738: board cards and list rows show a comment icon + number when comments_count > 0
// (nothing at 0), with the accessible name "N comments", and the number follows comments
// added or deleted in the task modal without a reload.

const baseTask = (id, title, extra = {}) => ({
  id,
  title,
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
  ...extra,
});

const setup = async (page) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page);
  const comments = [1, 2, 3].map((id) => ({
    id,
    message: `Comment ${id}`,
    user: { id: 1, name: 'Test User' },
    created_at: new Date(Date.UTC(2026, 8, 13, 10, 0, id)).toISOString(),
    updated_at: '2026-09-13T10:00:00Z',
    reactions: [],
  }));
  const tasks = () => [
    baseTask(1, 'Original task', { comments_count: comments.length }),
    baseTask(2, 'Silent task', { comments_count: 0 }),
    baseTask(3, 'Single comment task', { comments_count: 1 }),
    baseTask(4, 'Legacy task'),
  ];
  await page.route('**/api/tasks/status/1*', (route) =>
    route.fulfill({ json: { data: tasks() } }),
  );
  await page.route('**/api/tasks/current**', (route) =>
    route.fulfill({
      json: {
        data: tasks(),
        meta: {
          current_page: 1,
          per_page: 10,
          total: 4,
          last_page: 1,
          from: 1,
          to: 4,
        },
      },
    }),
  );
  await page.route('**/api/tasks/1', (route) =>
    route.request().method() === 'GET'
      ? route.fulfill({ json: { data: tasks()[0] } })
      : route.fallback(),
  );
  await page.route('**/api/tasks/1/comments**', (route) => {
    const req = route.request();
    if (req.method() === 'POST') {
      const added = {
        id: comments.length + 1,
        message: req.postDataJSON().message,
        user: { id: 1, name: 'Test User' },
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        reactions: [],
      };
      comments.push(added);
      return route.fulfill({ json: { data: added } });
    }
    return route.fulfill({ json: { data: comments } });
  });
  await page.route('**/api/comments/*', (route) => {
    if (route.request().method() !== 'DELETE') return route.fallback();
    comments.pop();
    return route.fulfill({ json: { data: null } });
  });
  page.on('dialog', (dialog) => dialog.accept());
  return { comments };
};

const badge = (row) => row.getByTestId('task-comment-count');

test.describe('Comment count on board cards and list rows', () => {
  test('board card shows the count, singular for one, nothing at zero', async ({
    page,
  }) => {
    await setup(page);
    await page.goto('/demo/board');
    const card = (id) => page.locator(`[data-task-id="${id}"]`);
    await expect(card(1)).toBeVisible();
    await expect(card(1).getByText('3 comments', { exact: true })).toHaveCount(
      1,
    );
    await expect(badge(card(1))).toHaveText(/3/);
    await expect(card(3).getByText('1 comment', { exact: true })).toHaveCount(
      1,
    );
    await expect(badge(card(2))).toHaveCount(0);
    await expect(badge(card(4))).toHaveCount(0);
  });

  test('list row shows the count outside the title button, nothing at zero', async ({
    page,
  }) => {
    await setup(page);
    await page.goto('/demo/list');
    const row = (id) => page.locator(`[data-task-id="${id}"]`).first();
    await expect(row(1)).toBeVisible();
    await expect(row(1).getByText('3 comments', { exact: true })).toHaveCount(
      1,
    );
    await expect(row(3).getByText('1 comment', { exact: true })).toHaveCount(1);
    await expect(badge(row(2))).toHaveCount(0);
    await expect(badge(row(4))).toHaveCount(0);
    await expect(
      row(1)
        .getByRole('button', { name: /Original task/ })
        .getByTestId('task-comment-count'),
    ).toHaveCount(0);
  });

  test('the list row follows a comment added and deleted in the modal', async ({
    page,
  }) => {
    await setup(page);
    await page.goto('/demo/list');
    const row = page.locator('[data-task-id="1"]').first();
    await expect(row.getByText('3 comments', { exact: true })).toHaveCount(1);
    await row.click();
    const form = page.locator('.new-form-container');
    await expect(form.locator('[data-comment-id]')).toHaveCount(3);

    const input = form.locator('footer textarea');
    await input.fill('a fourth comment');
    await input.press('Enter');
    await expect(form.locator('[data-comment-id]')).toHaveCount(4);
    await expect(row.getByText('4 comments', { exact: true })).toHaveCount(1);

    await form.locator('[data-comment-id="4"]').hover();
    await form
      .locator('[data-comment-id="4"]')
      .getByRole('button', { name: 'Delete comment' })
      .click();
    await expect(form.locator('[data-comment-id]')).toHaveCount(3);
    await expect(row.getByText('3 comments', { exact: true })).toHaveCount(1);
  });

  test('the board card follows a comment added in the modal and keeps it after navigating', async ({
    page,
  }) => {
    await setup(page);
    await page.goto('/demo/board');
    const card = page.locator('[data-task-id="1"]');
    await expect(card.getByText('3 comments', { exact: true })).toHaveCount(1);
    await card.getByRole('link', { name: 'Original task' }).click();
    const form = page.locator('.new-form-container');
    await expect(form.locator('[data-comment-id]')).toHaveCount(3);
    const input = form.locator('footer textarea');
    await input.fill('board comment');
    await input.press('Enter');
    await expect(card.getByText('4 comments', { exact: true })).toHaveCount(1);

    await page.keyboard.press('Escape');
    await expect(form).toHaveCount(0);
    await page.getByRole('link', { name: 'List', exact: true }).first().click();
    const row = page.locator('[data-task-id="1"]').first();
    await expect(row.getByText('4 comments', { exact: true })).toHaveCount(1);
  });

  test('a board visited before the comment shows the new count when revisited', async ({
    page,
  }) => {
    await setup(page);
    await page.goto('/demo/board');
    const card = page.locator('[data-task-id="1"]');
    await expect(card.getByText('3 comments', { exact: true })).toHaveCount(1);

    await page.getByRole('link', { name: 'List', exact: true }).first().click();
    const row = page.locator('[data-task-id="1"]').first();
    await row.click();
    const form = page.locator('.new-form-container');
    await expect(form.locator('[data-comment-id]')).toHaveCount(3);
    const input = form.locator('footer textarea');
    await input.fill('comment while the board is away');
    await input.press('Enter');
    await expect(row.getByText('4 comments', { exact: true })).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(form).toHaveCount(0);

    await page
      .getByRole('link', { name: 'Board', exact: true })
      .first()
      .click();
    await expect(
      page
        .locator('[data-task-id="1"]')
        .getByText('4 comments', { exact: true }),
    ).toHaveCount(1);
  });

  test('an edit saved after a comment keeps the new count', async ({
    page,
  }) => {
    await setup(page);
    await page.goto('/demo/list');
    const row = page.locator('[data-task-id="1"]').first();
    await row.click();
    const form = page.locator('.new-form-container');
    await expect(form.locator('[data-comment-id]')).toHaveCount(3);
    const input = form.locator('footer textarea');
    await input.fill('a fourth comment');
    await input.press('Enter');
    await expect(row.getByText('4 comments', { exact: true })).toHaveCount(1);

    await form.getByPlaceholder('Task name').fill('Renamed after comment');
    await expect(row.getByText('Renamed after comment')).toBeVisible();
    await expect(row.getByText('4 comments', { exact: true })).toHaveCount(1);
  });
});
