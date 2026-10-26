import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
});

const LONG_TITLE =
  `Long title ${'lorem ipsum dolor sit amet '.repeat(8)}`.slice(0, 200);

const task = (id, extra = {}) => ({
  id,
  title: `Row task ${id}`,
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
  deleted_at: null,
  created_at: '2026-09-13T10:00:00Z',
  updated_at: '2026-09-13T10:00:00Z',
  ...extra,
});

const openList = async (page, extraRoutes) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await mockApp(page);
  await extraRoutes?.();
  await page.route('**/api/tasks/current*', (route) =>
    route.fulfill({
      json: {
        data: [task(1), task(2, { title: LONG_TITLE })],
        meta: {
          current_page: 1,
          per_page: 10,
          total: 2,
          last_page: 1,
          from: 1,
          to: 2,
        },
      },
    }),
  );
  await page.goto('/demo/list');
  const row = page.locator('[data-task-id="1"]');
  await expect(row).toBeVisible();
  await expect(page.locator('[data-task-id="2"]')).toBeVisible();
  return { row };
};

test('a list row holds no nested buttons and is not itself a button', async ({
  page,
}) => {
  await openList(page);
  const rows = page.locator('[data-task-id]');
  expect(await rows.count()).toBeGreaterThan(1);
  expect(await page.locator('[data-task-id] button button').count()).toBe(0);
  expect(await page.locator('button[data-task-id]').count()).toBe(0);
  expect(await page.locator('[data-task-id] [data-task-id]').count()).toBe(0);
});

test('Tab through a row reaches open, assign, task actions and the timer in order', async ({
  page,
}) => {
  const { row } = await openList(page);
  await row.focus();
  const stops = [
    row.getByRole('button', { name: 'Row task 1' }),
    row.getByRole('button', { name: 'Assign someone' }),
    row.getByRole('button', { name: 'Task actions' }),
    row.getByRole('button', { name: 'Start timer' }),
  ];
  for (const stop of stops) {
    await page.keyboard.press('Tab');
    await expect(stop).toBeFocused();
  }
});

test('the timer button is named and at least 24x24', async ({ page }) => {
  const { row } = await openList(page);
  const start = row.getByRole('button', { name: 'Start timer' });
  const box = await start.boundingBox();
  expect(box.width).toBeGreaterThanOrEqual(24);
  expect(box.height).toBeGreaterThanOrEqual(24);

  await page.route(/\/api\/tasks\/1\/(start|stop)/, (route) =>
    route.fulfill({
      json: { data: task(1, { start_time: 1760000000 }) },
    }),
  );
  await page.route('**/api/tasks/current*', (route) =>
    route.fulfill({
      json: {
        data: [task(1, { start_time: Math.floor(Date.now() / 1000) })],
        meta: {
          current_page: 1,
          per_page: 10,
          total: 1,
          last_page: 1,
          from: 1,
          to: 1,
        },
      },
    }),
  );
  await page.reload();
  const stop = page
    .locator('[data-task-id="1"]')
    .getByRole('button', { name: 'Stop timer' });
  await expect(stop).toBeVisible();
  const stopBox = await stop.boundingBox();
  expect(stopBox.width).toBeGreaterThanOrEqual(24);
  expect(stopBox.height).toBeGreaterThanOrEqual(24);
});

const PANEL = '[data-name="task-side-panel"]';

test('a click on the row opens the task', async ({ page }) => {
  const { row } = await openList(page);
  await row.click({ position: { x: 600, y: 20 } });
  await expect(page.locator(PANEL)).toHaveCount(1);
});

test('ArrowDown then Enter on the list opens the focused row', async ({
  page,
}) => {
  await openList(page);
  await page.keyboard.press('ArrowDown');
  await expect(page.locator('[data-task-id="1"]')).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator(PANEL)).toHaveCount(1);
});

for (const key of ['Enter', 'Space']) {
  test(`${key} on the title button opens the task`, async ({ page }) => {
    const { row } = await openList(page);
    await row.getByRole('button', { name: 'Row task 1' }).focus();
    await page.keyboard.press(key);
    await expect(page.locator(PANEL)).toHaveCount(1);
  });
}

test('a 200-char title is not cut in JS: full text, CSS clamp, full title attribute', async ({
  page,
}) => {
  await openList(page);
  expect(LONG_TITLE).toHaveLength(200);
  const open = page
    .locator('[data-task-id="2"]')
    .getByRole('button', { name: LONG_TITLE });
  await expect(open).toHaveCount(1);
  await expect(open).toHaveAttribute('title', LONG_TITLE);
  expect(await open.textContent()).toBe(LONG_TITLE);
  const box = await open.boundingBox();
  expect(box.width).toBeGreaterThan(750);
  const clamp = await open.locator('span').evaluate((el) => ({
    lines: getComputedStyle(el).webkitLineClamp,
    overflow: getComputedStyle(el).overflow,
  }));
  expect(clamp).toEqual({ lines: '2', overflow: 'hidden' });
});

const outline = (locator) =>
  locator.evaluate((el) => {
    const style = getComputedStyle(el);
    return {
      visible: el.matches(':focus-visible'),
      style: style.outlineStyle,
      width: style.outlineWidth,
    };
  });

test('the list search has an accessible label', async ({ page }) => {
  await openList(page);
  await expect(
    page.getByRole('searchbox', { name: 'Search tasks' }),
  ).toHaveCount(1);
});

test('the sort select and direction button show the focus ring on Tab', async ({
  page,
}) => {
  await mockApp(page);
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto('/demo/archive');
  await page.getByRole('searchbox', { name: 'Search tasks' }).focus();
  await page.keyboard.press('Tab');
  const sort = page.getByTitle('Sort by date');
  await expect(sort).toBeFocused();
  expect(await outline(sort)).toEqual({
    visible: true,
    style: 'solid',
    width: '2px',
  });
  await page.keyboard.press('Tab');
  const direction = page.locator('button[title$=" first"]');
  await expect(direction).toBeFocused();
  expect(await outline(direction)).toEqual({
    visible: true,
    style: 'solid',
    width: '2px',
  });
});

test('the board filters select shows the focus ring on Tab', async ({
  page,
}) => {
  await mockApp(page);
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.route('**/api/project_categories*', (route) =>
    route.fulfill({
      json: {
        data: [
          { id: 1, name: 'Alpha', code: 'AL', color: '#ff0000', settings: [] },
          { id: 2, name: 'Beta', code: 'BE', color: '#00ff00', settings: [] },
        ],
      },
    }),
  );
  await page.goto('/demo/board');
  await page.getByRole('button', { name: /^Filters/ }).click();
  await page.keyboard.press('Tab');
  const select = page.locator('select').first();
  await select.focus();
  expect(await outline(select)).toEqual({
    visible: true,
    style: 'solid',
    width: '2px',
  });
});

test('each member avatar button is named after its user', async ({ page }) => {
  await openList(page, async () => {
    await page.route('**/api/workspaces/1/members*', (route) =>
      route.fulfill({
        json: {
          data: [
            { id: 1, name: 'Test User', email: 'test@example.test' },
            { id: 2, name: 'Ann Lee', email: 'ann@example.test' },
          ],
        },
      }),
    );
  });
  const members = page.getByRole('button', { name: /workspace members/i });
  await expect(members).toHaveCount(2);
  const ann = page.getByRole('button', { name: 'Ann Lee – workspace members' });
  await expect(ann).toHaveCount(1);
  await expect(ann.locator('span[title="Ann Lee"]')).toBeVisible();
  await expect(
    page.locator('button:has(span[title="Ann Lee"]):visible'),
  ).toHaveCount(1);
});

const statuses = [
  { id: 1, name: 'Backlog', type: 'default', color: '#888888' },
  { id: 2, name: 'Done', type: 'completed', color: '#16a34a' },
  { id: 9, name: 'Archive', type: 'archived', color: '#888888' },
].map((status, i) => ({ ...status, pivot: { order: i + 1, is_active: true } }));

const openBoard = async (
  page,
  { withArchive = true, doneTasks = 3, archiveStatus } = {},
) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await mockApp(page, { taskCount: 1 });
  const list = withArchive ? statuses : statuses.filter((s) => s.id !== 9);
  await page.route(/\/api\/(workspaces\/)?statuses$/, (route) =>
    route.fulfill({ json: { data: list } }),
  );
  await page.route(/\/api\/tasks\/status\/2/, (route) =>
    route.fulfill({
      json: {
        data: Array.from({ length: doneTasks }, (_, i) =>
          task(10 + i, { status_id: 2, title: `Done task ${i + 1}` }),
        ),
      },
    }),
  );
  const puts = [];
  await page.route(/\/api\/statuses\/\d+\/tasks$/, (route) => {
    puts.push(
      `${route.request().method()} ${new URL(route.request().url()).pathname}`,
    );
    if (archiveStatus && archiveStatus !== 200)
      return route.fulfill({
        status: archiveStatus,
        json: { message: 'boom' },
      });
    return route.fulfill({ json: { data: [] } });
  });
  await page.goto('/demo/board');
  await expect(page.getByText('Done', { exact: true }).first()).toBeVisible();
  return { puts };
};

const clickArchiveAll = async (page) => {
  const doneHeader = page
    .locator('[data-tour="board.add-in-column"]')
    .nth(1)
    .locator('xpath=..');
  await doneHeader.getByRole('button', { name: 'Open options' }).click();
  await page.getByRole('menuitem', { name: 'Archive all' }).click();
};

test('Archive all confirms with the task count; Cancel sends nothing, Ok sends one PUT', async ({
  page,
}) => {
  const { puts } = await openBoard(page);
  await expect(page.getByText('Done task 3')).toBeVisible();
  await clickArchiveAll(page);
  await expect(page.getByText('Archive 3 tasks?')).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByText('Archive 3 tasks?')).toHaveCount(0);
  expect(puts).toEqual([]);

  await clickArchiveAll(page);
  await page.getByRole('button', { name: 'Ok' }).click();
  await expect.poll(() => puts).toEqual(['PUT /api/statuses/9/tasks']);
});

test('Archive all shows a toast when the request fails', async ({ page }) => {
  const { puts } = await openBoard(page, { archiveStatus: 500 });
  await clickArchiveAll(page);
  await page.getByRole('button', { name: 'Ok' }).click();
  await expect(
    page.getByText('Failed to archive tasks', { exact: true }).first(),
  ).toBeVisible();
  expect(puts.length).toBeGreaterThanOrEqual(1);
  await expect(page.getByText('Archive 3 tasks?')).toHaveCount(0);
});

test('Archive all without an archive status shows a toast and sends nothing', async ({
  page,
}) => {
  const { puts } = await openBoard(page, { withArchive: false });
  await clickArchiveAll(page);
  await expect(
    page.getByText('Archive status not found', { exact: true }).first(),
  ).toBeVisible();
  await expect(page.getByText('Archive 3 tasks?')).toHaveCount(0);
  expect(puts).toEqual([]);
});

test('Archive all on an empty column sends nothing and says so', async ({
  page,
}) => {
  const { puts } = await openBoard(page, { doneTasks: 0 });
  await clickArchiveAll(page);
  await expect(
    page
      .getByText('No tasks to archive in this column', { exact: true })
      .first(),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Ok' })).toHaveCount(0);
  expect(puts).toEqual([]);
});

test('the colour picker closes from its Close button with the keyboard', async ({
  page,
}) => {
  await openBoard(page);
  const doneHeader = page
    .locator('[data-tour="board.add-in-column"]')
    .nth(1)
    .locator('xpath=..');
  await doneHeader.getByRole('button', { name: 'Open options' }).click();
  await page.getByRole('menuitem', { name: 'Edit status' }).click();
  await page.locator('button.w-2\\/4').click();
  const close = page.getByRole('button', { name: 'Close', exact: true });
  await expect(close).toBeVisible();
  await close.focus();
  await page.keyboard.press('Enter');
  await expect(close).toHaveCount(0);
  await expect(page.getByText('Status color :')).toBeVisible();
});

test('single-card Archive reports a failure with a toast, not alert()', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { taskCount: 3 });
  await page.route(/\/api\/(workspaces\/)?statuses$/, (route) =>
    route.fulfill({
      json: { data: statuses.filter((status) => status.id !== 2) },
    }),
  );
  const nativeDialogs = [];
  page.on('dialog', (dialog) => {
    nativeDialogs.push(dialog.message());
    return dialog.dismiss();
  });
  await page.route(/\/api\/tasks\/2\/9$/, (route) =>
    route.fulfill({ status: 500, json: { message: 'boom' } }),
  );
  await page.goto('/demo/board');
  const card = page.locator('[data-task-id="2"]');
  await card.waitFor();
  await card.hover();
  await card.getByRole('button', { name: 'Task actions' }).click();
  await page.getByRole('menuitem', { name: 'Archive' }).click();
  await page.getByRole('button', { name: 'Ok' }).click();
  await expect(
    page.getByText('Failed to archive task', { exact: true }).first(),
  ).toBeVisible();
  expect(nativeDialogs).toEqual([]);
});
