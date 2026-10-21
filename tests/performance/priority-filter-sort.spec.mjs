import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';

import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

const shotsDir = process.env.TM715_SHOTS_DIR;
if (shotsDir) mkdirSync(shotsDir, { recursive: true });
const shot = async (page, name) => {
  if (!shotsDir) return;
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(shotsDir, name) });
};

const makeTask = (id, statusId, priority, title) => ({
  id,
  title,
  description: null,
  description_json: null,
  status: 'created',
  status_id: statusId,
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
  priority,
  deleted_at: null,
  created_at: '2026-09-13T10:00:00Z',
  updated_at: '2026-09-13T10:00:00Z',
});

// Manual (server) order per column. 14 has no priority and counts as medium.
const BOARD = {
  1: [
    makeTask(11, 1, 'low', 'Low A'),
    makeTask(12, 1, 'high', 'High A'),
    makeTask(13, 1, 'urgent', 'Urgent A'),
    makeTask(14, 1, null, 'No priority'),
    makeTask(15, 1, 'high', 'High B'),
    makeTask(16, 1, 'medium', 'Medium A'),
  ],
  2: [
    makeTask(21, 2, 'medium', 'Medium B'),
    makeTask(22, 2, 'urgent', 'Urgent B'),
  ],
};
const MANUAL_1 = [11, 12, 13, 14, 15, 16];
const SORTED_1 = [13, 12, 15, 14, 16, 11];

const isOrders = (entry) => entry.includes('/api/tasks/update-orders');

const prepare = async (
  page,
  { board = BOARD, list = null, viewport = { width: 1280, height: 800 } } = {},
) => {
  await page.setViewportSize(viewport);
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page, { multiStatus: true });
  const requests = [];
  const errors = [];
  page.on('request', (request) => {
    const url = new URL(request.url());
    if (url.pathname.startsWith('/api/'))
      requests.push(`${request.method()} ${url.pathname}${url.search}`);
  });
  page.on('pageerror', (error) => errors.push(error.message));
  const consoleErrors = [];
  page.on('console', (message) => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  });
  await page.route(
    (url) => url.pathname.startsWith('/api/tasks/status/'),
    (route) => {
      const id = new URL(route.request().url()).pathname.split('/').pop();
      return route.fulfill({ json: { data: board[id] ?? [] } });
    },
  );
  if (list)
    await page.route(
      (url) => url.pathname === '/api/tasks/current',
      (route) => {
        const params = new URL(route.request().url()).searchParams;
        const perPage = Number(params.get('per_page') || 10);
        const current = Number(params.get('page') || 1);
        const data = list.slice((current - 1) * perPage, current * perPage);
        return route.fulfill({
          json: {
            data,
            meta: {
              current_page: current,
              per_page: perPage,
              total: list.length,
              last_page: Math.max(1, Math.ceil(list.length / perPage)),
              from: data.length ? (current - 1) * perPage + 1 : 0,
              to: (current - 1) * perPage + data.length,
            },
          },
        });
      },
    );
  return { requests, errors, consoleErrors };
};

const column = (page, status) =>
  page.locator(`.board-card-draggable[data-status="${status}"]`);
const card = (page, status, id) =>
  column(page, status).locator(`:scope > [data-task-id="${id}"]`);
const columnIds = (page, status) =>
  column(page, status)
    .locator(':scope > [data-task-id]')
    .evaluateAll((nodes) => nodes.map((node) => Number(node.dataset.taskId)));

const columnCount = (page, title) =>
  page
    .locator('div.group.relative.mb-2', { hasText: title })
    .locator('span.tabular-nums')
    .first();

const openBoard = async (page, query = '', { cards = true } = {}) => {
  await page.goto(`/demo/board${query}`);
  await expect(column(page, 1)).toBeVisible();
  if (cards) await expect(page.locator('[data-task-id]').first()).toBeVisible();
};

const openFilters = async (page) => {
  await page.getByRole('button', { name: /^Filters/ }).click();
  const group = page.getByRole('group', { name: 'Priority' });
  await expect(group).toBeVisible();
  return group;
};

const dragHandle = async (page, from) => {
  const handle = from.locator('.task-drag-handle').first();
  await handle.click({ trial: true });
  const box = await handle.boundingBox();
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 15, box.y + 15, { steps: 5 });
  await expect(page.locator('.sortable-fallback')).toBeVisible();
};

// Drops `from` onto the upper half of `onto` (same column) — would place it before `onto`.
const dragAbove = async (page, from, onto) => {
  await dragHandle(page, from);
  const target = await onto.boundingBox();
  await page.mouse.move(target.x + target.width / 2, target.y + 4, {
    steps: 20,
  });
  await page.mouse.move(target.x + target.width / 2, target.y + 2, {
    steps: 3,
  });
  await page.mouse.up();
};

test.describe('board', () => {
  test('filters by several levels, counts follow, reload keeps it, Clear filters clears it', async ({
    page,
  }) => {
    const { requests, errors } = await prepare(page);
    await openBoard(page);
    expect(await columnIds(page, 1)).toEqual(MANUAL_1);
    await expect(columnCount(page, 'Backlog')).toHaveText('6');
    await expect(columnCount(page, 'In progress')).toHaveText('2');

    const group = await openFilters(page);
    await group.getByRole('button', { name: 'Urgent' }).click();
    await group.getByRole('button', { name: 'High' }).click();
    await expect(group.getByRole('button', { name: 'Urgent' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await expect(group.getByRole('button', { name: 'Low' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
    await expect(page).toHaveURL(
      /[?&]priority=urgent%2Chigh(&|$)|priority=urgent,high/,
    );
    await expect.poll(() => columnIds(page, 1)).toEqual([12, 13, 15]);
    await expect.poll(() => columnIds(page, 2)).toEqual([22]);
    await expect(columnCount(page, 'Backlog')).toHaveText('3');
    await expect(columnCount(page, 'In progress')).toHaveText('1');
    await shot(page, 'board-popover-light.png');

    await page.reload();
    await expect.poll(() => columnIds(page, 1)).toEqual([12, 13, 15]);
    await expect.poll(() => columnIds(page, 2)).toEqual([22]);

    // Medium includes the task without a priority.
    const reopened = await openFilters(page);
    await expect(
      reopened.getByRole('button', { name: 'High' }),
    ).toHaveAttribute('aria-pressed', 'true');
    await reopened.getByRole('button', { name: 'Urgent' }).click();
    await reopened.getByRole('button', { name: 'High' }).click();
    await reopened.getByRole('button', { name: 'Medium' }).click();
    await expect.poll(() => columnIds(page, 1)).toEqual([14, 16]);
    await expect.poll(() => columnIds(page, 2)).toEqual([21]);
    await page.keyboard.press('Escape');

    await page.getByTitle('Clear filters').click();
    await expect.poll(() => columnIds(page, 1)).toEqual(MANUAL_1);
    await expect(page).not.toHaveURL(/priority=/);
    expect(requests.filter(isOrders)).toEqual([]);
    expect(errors).toEqual([]);
  });

  test('sorts Urgent→High→Medium→Low with stable ties, survives reload, off restores manual order', async ({
    page,
  }) => {
    const { requests, errors } = await prepare(page);
    await openBoard(page);
    await openFilters(page);
    await page.getByLabel('Sort by priority').check();
    await expect(page).toHaveURL(/priority_sort=1/);
    await expect.poll(() => columnIds(page, 1)).toEqual(SORTED_1);
    await expect.poll(() => columnIds(page, 2)).toEqual([22, 21]);

    await page.reload();
    await expect.poll(() => columnIds(page, 1)).toEqual(SORTED_1);
    await openFilters(page);
    await expect(page.getByLabel('Sort by priority')).toBeChecked();

    // Sort + filter combine; Clear filters keeps the sort (D-007).
    await page
      .getByRole('group', { name: 'Priority' })
      .getByRole('button', { name: 'Low' })
      .click();
    await page
      .getByRole('group', { name: 'Priority' })
      .getByRole('button', { name: 'Urgent' })
      .click();
    await expect.poll(() => columnIds(page, 1)).toEqual([13, 11]);
    await page.keyboard.press('Escape');
    await page.getByTitle('Clear filters').click();
    await expect(page).not.toHaveURL(/priority=/);
    await expect(page).toHaveURL(/priority_sort=1/);
    await expect.poll(() => columnIds(page, 1)).toEqual(SORTED_1);

    await openFilters(page);
    await page.getByLabel('Sort by priority').uncheck();
    await expect(page).not.toHaveURL(/priority_sort/);
    await expect.poll(() => columnIds(page, 1)).toEqual(MANUAL_1);
    await expect.poll(() => columnIds(page, 2)).toEqual([21, 22]);
    expect(requests.filter(isOrders)).toEqual([]);
    expect(errors).toEqual([]);
  });

  test('ignores a garbage priority in the URL', async ({ page }) => {
    const { errors } = await prepare(page);
    await openBoard(page, '?priority=foo');
    await expect.poll(() => columnIds(page, 1)).toEqual(MANUAL_1);
    await expect.poll(() => columnIds(page, 2)).toEqual([21, 22]);
    const group = await openFilters(page);
    for (const name of ['Urgent', 'High', 'Medium', 'Low'])
      await expect(group.getByRole('button', { name })).toHaveAttribute(
        'aria-pressed',
        'false',
      );
    await expect(page.getByTitle('Clear filters')).toHaveCount(0);

    await openBoard(page, '?priority=foo,high');
    await expect.poll(() => columnIds(page, 1)).toEqual([12, 15]);
    expect(errors).toEqual([]);
  });

  test('levels with no match leave every column empty without errors', async ({
    page,
  }) => {
    const onlyMedium = {
      1: [makeTask(31, 1, 'medium', 'Plain 1')],
      2: [makeTask(32, 2, null, 'Plain 2')],
    };
    const { errors, consoleErrors } = await prepare(page, {
      board: onlyMedium,
    });
    await openBoard(page, '?priority=urgent,low', { cards: false });
    await expect(page.getByRole('button', { name: 'Add task' })).toHaveCount(2);
    await expect.poll(() => columnIds(page, 1)).toEqual([]);
    await expect.poll(() => columnIds(page, 2)).toEqual([]);
    await expect(page.getByText('Could not load board')).toHaveCount(0);
    expect(consoleErrors).toEqual([]);
    await page.getByTitle('Clear filters').click();
    await expect.poll(() => columnIds(page, 1)).toEqual([31]);
    await expect.poll(() => columnIds(page, 2)).toEqual([32]);
    expect(errors).toEqual([]);
  });

  test('narrow viewport: the filters modal has the priority controls', async ({
    page,
  }) => {
    const { errors } = await prepare(page, {
      viewport: { width: 390, height: 844 },
    });
    await openBoard(page);
    await page.getByTitle('Filters', { exact: true }).click();
    const group = page.getByRole('group', { name: 'Priority' });
    await expect(group).toBeVisible();
    await group.getByRole('button', { name: 'Urgent' }).click();
    await page.getByLabel('Sort by priority').check();
    await expect(page).toHaveURL(/priority=urgent/);
    await expect(page).toHaveURL(/priority_sort=1/);
    await shot(page, 'board-mobile-modal-light.png');
    await page.getByRole('button', { name: 'Clear filters' }).click();
    await expect(page).not.toHaveURL(/priority=/);
    await expect(page).toHaveURL(/priority_sort=1/);
    expect(errors).toEqual([]);
  });
});

test.describe('board order while sorted (criterion 3)', () => {
  test('control: unsorted, a within-column drag and Move to top save orders', async ({
    page,
  }) => {
    const { requests } = await prepare(page);
    await openBoard(page);
    await dragAbove(page, card(page, 1, 15), card(page, 1, 12));
    await expect
      .poll(() => columnIds(page, 1))
      .toEqual([11, 15, 12, 13, 14, 16]);
    await expect.poll(() => requests.filter(isOrders).length).toBe(1);

    await card(page, 1, 16)
      .getByRole('button', { name: 'Task actions' })
      .click();
    await page.getByRole('menuitem', { name: 'Move to top' }).click();
    await expect.poll(() => requests.filter(isOrders).length).toBe(2);
  });

  test('sorted from the URL: tie drag keeps order, cross-column drop changes only status', async ({
    page,
  }) => {
    const { requests, errors } = await prepare(page);
    await openBoard(page, '?priority_sort=1');
    await expect.poll(() => columnIds(page, 1)).toEqual(SORTED_1);

    // 15 and 12 are both High: a tie, so a re-sort cannot hide a reorder.
    await dragAbove(page, card(page, 1, 15), card(page, 1, 12));
    await page.waitForTimeout(500);
    expect(await columnIds(page, 1)).toEqual(SORTED_1);

    const statusSaved = page.waitForRequest(
      (request) =>
        request.method() === 'PUT' && request.url().endsWith('/api/tasks/11/2'),
    );
    await dragHandle(page, card(page, 1, 11));
    const target = await column(page, 2).boundingBox();
    await page.mouse.move(target.x + target.width / 2, target.y + 35, {
      steps: 20,
    });
    await page.mouse.up();
    await statusSaved;
    await expect.poll(() => columnIds(page, 2)).toEqual([22, 21, 11]);
    await page.waitForTimeout(500);
    expect(requests.filter(isOrders)).toEqual([]);
    expect(errors).toEqual([]);
  });

  test('sorted at runtime: tie drag keeps order, Move to top/bottom save nothing', async ({
    page,
  }) => {
    const { requests, errors } = await prepare(page);
    await openBoard(page);
    await openFilters(page);
    await page.getByLabel('Sort by priority').check();
    await page.keyboard.press('Escape');
    await expect.poll(() => columnIds(page, 1)).toEqual(SORTED_1);

    await dragAbove(page, card(page, 1, 15), card(page, 1, 12));
    await page.waitForTimeout(500);
    expect(await columnIds(page, 1)).toEqual(SORTED_1);

    await card(page, 1, 16)
      .getByRole('button', { name: 'Task actions' })
      .click();
    await page.getByRole('menuitem', { name: 'Move to top' }).click();
    await card(page, 1, 13)
      .getByRole('button', { name: 'Task actions' })
      .click();
    await page.getByRole('menuitem', { name: 'Move to bottom' }).click();
    await page.waitForTimeout(500);
    expect(await columnIds(page, 1)).toEqual(SORTED_1);
    expect(requests.filter(isOrders)).toEqual([]);
    expect(errors).toEqual([]);
  });
});

// 30 tasks over 3 server pages. The only urgent task (125) is on server page 3.
const LIST = Array.from({ length: 30 }, (_, index) => {
  const i = index + 1;
  let priority;
  if (i === 25) priority = 'urgent';
  else if (i % 3 === 0) priority = 'high';
  else if (i % 3 === 1) priority = 'low';
  else priority = i % 6 === 2 ? null : 'medium';
  return makeTask(100 + i, 1, priority, `List task ${100 + i}`);
});
const HIGH_IDS = LIST.filter((t) => t.priority === 'high').map((t) => t.id);

const listIds = (page) =>
  page
    .locator('[data-task-id]')
    .evaluateAll((nodes) => [
      ...new Set(nodes.map((node) => Number(node.dataset.taskId))),
    ]);

const openList = async (page, query = '') => {
  await page.goto(`/demo/list${query}`);
  await expect(page.locator('[data-task-id]').first()).toBeVisible();
};

test.describe('active list', () => {
  test('filter and sort apply across all pages, totals follow, reload keeps them', async ({
    page,
  }) => {
    const { requests, errors } = await prepare(page, { list: LIST });
    await openList(page);
    await expect(page.getByText('Showing 1 to 10 of 30 tasks')).toBeVisible();

    await page.getByTitle('filters').click();
    const group = page.getByRole('group', { name: 'Priority' });
    await expect(group).toBeVisible();
    const before = requests.length;
    await group.getByRole('button', { name: 'Urgent' }).click();
    await group.getByRole('button', { name: 'High' }).click();
    await shot(page, 'list-dialog-light.png');
    await page.keyboard.press('Escape');
    await expect(page).toHaveURL(/priority=urgent(%2C|,)high/);

    // Filter only: server order kept, 11 matches → 2 local pages.
    const filtered = LIST.filter((t) =>
      ['urgent', 'high'].includes(t.priority),
    ).map((t) => t.id);
    await expect(page.getByText('Showing 1 to 10 of 11 tasks')).toBeVisible();
    await expect.poll(() => listIds(page)).toEqual(filtered.slice(0, 10));
    await page.getByRole('button', { name: 'Next' }).click();
    await expect(page.getByText('Showing 11 to 11 of 11 tasks')).toBeVisible();
    await expect.poll(() => listIds(page)).toEqual(filtered.slice(10));
    const listCalls = requests
      .slice(before)
      .filter((entry) => entry.startsWith('GET /api/tasks/current'));
    expect(listCalls.length).toBeGreaterThan(0);
    for (const call of listCalls) expect(call).toContain('per_page=100000');

    // Sort: the urgent task from server page 3 leads page 1.
    await page.getByTitle('filters').click();
    await page.getByLabel('Sort by priority').check();
    await page.keyboard.press('Escape');
    await expect(page).toHaveURL(/priority_sort=1/);
    await expect(page.getByText('Showing 1 to 10 of 11 tasks')).toBeVisible();
    await expect
      .poll(() => listIds(page))
      .toEqual([125, ...HIGH_IDS.slice(0, 9)]);

    await page.reload();
    await expect(page.getByText('Showing 1 to 10 of 11 tasks')).toBeVisible();
    await expect
      .poll(() => listIds(page))
      .toEqual([125, ...HIGH_IDS.slice(0, 9)]);
    expect(errors).toEqual([]);
  });

  test('sort without a filter pulls the urgent task from the last server page', async ({
    page,
  }) => {
    const { errors } = await prepare(page, { list: LIST });
    await openList(page, '?priority_sort=1');
    await expect(page.getByText('Showing 1 to 10 of 30 tasks')).toBeVisible();
    await expect
      .poll(() => listIds(page))
      .toEqual([125, ...HIGH_IDS.slice(0, 9)]);
    await openList(page, '?priority=foo');
    await expect(page.getByText('Showing 1 to 10 of 30 tasks')).toBeVisible();
    await expect
      .poll(() => listIds(page))
      .toEqual(LIST.slice(0, 10).map((t) => t.id));
    expect(errors).toEqual([]);
  });

  test('realtime create/update respects the priority filter', async ({
    page,
  }) => {
    await prepare(page, { list: LIST });
    await openList(page, '?priority=urgent,high');
    await expect(page.getByText(/of 11 tasks/)).toBeVisible();
    const commit = (mutation, payload) =>
      page.evaluate(
        ([name, value]) =>
          document
            .querySelector('#app')
            .__vue_app__.config.globalProperties.$store.commit(name, value),
        [mutation, payload],
      );
    await commit('taskCreated', makeTask(901, 1, 'medium', 'Live medium'));
    await commit('taskCreated', makeTask(902, 1, null, 'Live bare'));
    await page.waitForTimeout(300);
    expect(await listIds(page)).not.toContain(901);
    expect(await listIds(page)).not.toContain(902);
    await expect(page.getByText(/of 11 tasks/)).toBeVisible();

    await commit('taskCreated', makeTask(903, 1, 'urgent', 'Live urgent'));
    await expect.poll(() => listIds(page)).toContain(903);
    await expect(page.getByText(/of 12 tasks/)).toBeVisible();

    await commit('updateSingleTask', { ...LIST[2], priority: 'low' });
    await expect.poll(() => listIds(page)).not.toContain(103);
    await expect(page.getByText(/of 11 tasks/)).toBeVisible();
  });

  test('no match shows the empty state, Clear filters drops priority and keeps the sort', async ({
    page,
  }) => {
    const plain = [
      makeTask(201, 1, 'medium', 'Plain'),
      makeTask(202, 1, null, 'Bare'),
    ];
    const { errors, consoleErrors } = await prepare(page, { list: plain });
    await page.goto('/demo/list?priority=urgent&priority_sort=1');
    await expect(page.getByText('No tasks match your filters')).toBeVisible();
    expect(consoleErrors).toEqual([]);
    await page.getByRole('button', { name: 'Clear filters' }).click();
    await expect(page).not.toHaveURL(/priority=/);
    await expect(page).toHaveURL(/priority_sort=1/);
    await expect.poll(() => listIds(page)).toEqual([201, 202]);
    expect(errors).toEqual([]);
  });
});

test.describe('dark theme', () => {
  test.skip(!shotsDir, 'screenshots only (TM715_SHOTS_DIR)');
  test('board popover, mobile modal and list dialog in dark', async ({
    page,
  }) => {
    await page.addInitScript(() => localStorage.setItem('colorScheme', 'dark'));
    await prepare(page, { list: LIST });
    await openBoard(page, '?priority=urgent,high&priority_sort=1');
    await openFilters(page);
    await shot(page, 'board-popover-dark.png');
    await page.keyboard.press('Escape');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(page.locator('[data-task-id]').first()).toBeVisible();
    await page.getByTitle('Filters', { exact: true }).click();
    await expect(page.getByRole('group', { name: 'Priority' })).toBeVisible();
    await page.waitForTimeout(1000);
    await shot(page, 'board-mobile-modal-dark.png');
    await page.setViewportSize({ width: 1280, height: 800 });
    await openList(page, '?priority=high&priority_sort=1');
    await page.getByTitle('filters').click();
    await expect(page.getByRole('group', { name: 'Priority' })).toBeVisible();
    await shot(page, 'list-dialog-dark.png');
  });
});

test.describe('local workspace (criterion 8)', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      sessionStorage.setItem('tmgr:tours:auto', 'true');
      localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
    });
  });

  test('filter and sort work on the local board and list', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    const shell = await desktopPage(
      page,
      { account: false },
      {
        profile: {
          id: -1,
          uuid: '00000000-0000-4000-8000-000000000001',
          name: 'Ada',
          created_at: '2026-10-11T10:00:00Z',
        },
      },
    );
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.goto('/');
    await expect(page).toHaveURL(/\/local-my-workspace\//);
    await page.goto('/local-my-workspace/board');
    for (const title of ['Loc low', 'Loc urgent', 'Loc plain', 'Loc high']) {
      await page.goto('/local-my-workspace/board');
      await page
        .getByRole('button', { name: 'New task', exact: true })
        .first()
        .click();
      await page.getByPlaceholder('Task name').fill(title);
      await page.keyboard.press('ControlOrMeta+KeyS');
      await expect(page.getByText(title).first()).toBeVisible();
    }
    const rows = () =>
      shell('local_db_select', {
        code: 'my-workspace',
        sql: 'SELECT id, title, priority, status_id FROM tasks ORDER BY id',
        params: [],
      });
    await expect.poll(async () => (await rows()).length).toBe(4);
    for (const [title, priority] of [
      ['Loc low', 'low'],
      ['Loc urgent', 'urgent'],
      ['Loc high', 'high'],
    ])
      await shell('local_db_execute', {
        code: 'my-workspace',
        sql: 'UPDATE tasks SET priority = ? WHERE title = ?',
        params: [priority, title],
      });
    const byTitle = Object.fromEntries(
      (await rows()).map((row) => [row.title, row]),
    );
    expect(byTitle['Loc plain'].priority).toBe('medium');
    const statusId = byTitle['Loc low'].status_id;
    const id = (title) => byTitle[title].id;

    await page.goto('/local-my-workspace/board');
    await expect(card(page, statusId, id('Loc low'))).toBeVisible();
    const group = await openFilters(page);
    await page.getByLabel('Sort by priority').check();
    await expect
      .poll(() => columnIds(page, statusId))
      .toEqual([
        id('Loc urgent'),
        id('Loc high'),
        id('Loc plain'),
        id('Loc low'),
      ]);
    await group.getByRole('button', { name: 'Urgent' }).click();
    await group.getByRole('button', { name: 'Medium' }).click();
    await expect
      .poll(() => columnIds(page, statusId))
      .toEqual([id('Loc urgent'), id('Loc plain')]);
    await shot(page, 'local-board-popover.png');

    await page.goto(
      '/local-my-workspace/list?priority=urgent,high&priority_sort=1',
    );
    await expect
      .poll(() => listIds(page))
      .toEqual([id('Loc urgent'), id('Loc high')]);
    await page.getByTitle('filters').click();
    await expect(page.getByRole('group', { name: 'Priority' })).toBeVisible();
    expect(errors).toEqual([]);
  });
});
