import { expect, test } from '@playwright/test';

const WORKSPACES = [
  { id: 1, name: 'Demo', code: 'demo', user_id: 1, is_default: true },
  { id: 2, name: 'Other', code: 'other', user_id: 1, is_default: false },
];

const STATUSES = {
  1: [
    { id: 1, name: 'Backlog', type: 'default' },
    { id: 2, name: 'In progress', type: 'active' },
  ],
  2: [
    { id: 3, name: 'Todo', type: 'default' },
    { id: 4, name: 'Doing', type: 'active' },
    { id: 5, name: 'Done', type: 'completed' },
  ],
};

async function mockTwoWorkspaces(page) {
  await page.addInitScript(() =>
    localStorage.setItem('token', JSON.stringify({ token: 'fixture' })),
  );
  await page.routeWebSocket(/.*/, () => {});
  let currentWorkspace = 1;
  const user = () => ({
    id: 1,
    name: 'Test User',
    email: 'test@example.test',
    settings: [
      { id: 5, key: 'current_workspace', value: String(currentWorkspace) },
    ],
  });
  const task = (id, statusId) => ({
    id,
    title: `Task ${id} in ws ${currentWorkspace}`,
    status: 'created',
    status_id: statusId,
    workspace_id: currentWorkspace,
    user_id: 1,
    assignees: [],
    settings: [],
    checkpoints: [],
    common_time: 0,
    start_time: 0,
    approximately_time: 0,
    project_category_id: null,
    created_at: '2026-09-13T10:00:00Z',
    updated_at: '2026-09-13T10:00:00Z',
  });
  await page.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/', '');
    let data = [];
    if (path === 'v2/user/settings' && route.request().method() === 'PUT') {
      const setting = route
        .request()
        .postDataJSON()
        .find((s) => s.id === 5);
      if (setting) currentWorkspace = Number(setting.value);
      data = user();
    } else if (path === 'user') data = user();
    else if (path === 'workspaces') data = WORKSPACES;
    else if (path.endsWith('feature-toggles'))
      data = path.startsWith('user')
        ? { default_landing_page: { value: 'board' } }
        : { board: { enabled: true } };
    else if (path.endsWith('statuses'))
      data = STATUSES[currentWorkspace].map((s, i) => ({
        ...s,
        color: '#888888',
        pivot: { order: i + 1, is_active: true },
      }));
    else if (path.endsWith('members')) data = [user()];
    else if (path.startsWith('tasks/status/')) {
      const statusId = Number(path.split('/')[2]);
      data = [task(statusId * 10 + 1, statusId), task(statusId * 10 + 2, statusId)];
    } else if (path === 'tasks/current')
      data = [task(1, STATUSES[currentWorkspace][0].id)];
    await route.fulfill({ json: { data, meta: { current_page: 1, last_page: 1 } } });
  });
}

test('switching workspace on the board keeps the app rendering', async ({
  page,
}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error' || /\[Vue warn\]/.test(message.text()))
      errors.push(message.text());
  });

  await mockTwoWorkspaces(page);
  await page.goto('/demo/board');
  await expect(page.getByText('Task 11 in ws 1').first()).toBeVisible();

  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /Other/ }).first().click();
  await expect(page).toHaveURL(/\/other\/board/);
  await expect(page.getByText('Task 31 in ws 2').first()).toBeVisible();
  await expect(page.getByText('Task 11 in ws 1').first()).toBeHidden();
  expect(errors.filter((e) => /Unhandled error/.test(e))).toEqual([]);

  await page.goto('/other/list');
  await page.goto('/other/board');
  await expect(page.getByText('Task 31 in ws 2').first()).toBeVisible();

  expect(
    errors.filter((e) => /parentNode|bum|Unhandled error|runtime-15/.test(e)),
  ).toEqual([]);
});
