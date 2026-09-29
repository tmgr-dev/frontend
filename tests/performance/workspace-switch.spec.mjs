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

// The account default never moves on a plain switch: the server always reports workspace 1,
// and every workspace-scoped response is picked by the X-Workspace-Id header instead.
const user = () => ({
  id: 1,
  name: 'Test User',
  email: 'test@example.test',
  settings: [{ id: 5, key: 'current_workspace', value: '1' }],
});

const task = (id, statusId, workspaceId) => ({
  id,
  title: `Task ${id} in ws ${workspaceId}`,
  status: 'created',
  status_id: statusId,
  workspace_id: workspaceId,
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

async function routeApi(context, requests) {
  await context.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/', '');
    const headers = route.request().headers();
    const workspaceId = Number(headers['x-workspace-id']) || 1;
    if (requests) requests.push({ path, method: route.request().method(), workspaceId });
    let data = [];
    if (path === 'v2/user/settings' && route.request().method() === 'PUT') {
      // A settings save (e.g. "Default workspace") still reaches the server, but must never be
      // what moves this test's tab — respond with the unchanged account.
      data = user();
    } else if (path === 'user') data = user();
    else if (path === 'workspaces') data = WORKSPACES;
    else if (path.endsWith('feature-toggles'))
      data = path.startsWith('user')
        ? { default_landing_page: { value: 'board' } }
        : { board: { enabled: true } };
    else if (path.endsWith('statuses'))
      data = STATUSES[workspaceId].map((s, i) => ({
        ...s,
        color: '#888888',
        pivot: { order: i + 1, is_active: true },
      }));
    else if (path.endsWith('members')) data = [user()];
    else if (path.startsWith('tasks/status/')) {
      const statusId = Number(path.split('/')[2]);
      data = [
        task(statusId * 10 + 1, statusId, workspaceId),
        task(statusId * 10 + 2, statusId, workspaceId),
      ];
    } else if (path === 'tasks/current')
      data = [task(1, STATUSES[workspaceId][0].id, workspaceId)];
    await route.fulfill({ json: { data, meta: { current_page: 1, last_page: 1 } } });
  });
}

async function mockTwoWorkspaces(page, requests) {
  await page.addInitScript(() =>
    localStorage.setItem('token', JSON.stringify({ token: 'fixture' })),
  );
  await page.routeWebSocket(/.*/, () => {});
  await routeApi(page, requests);
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

  const requests = [];
  await mockTwoWorkspaces(page, requests);
  await page.goto('/demo/board');
  await expect(page.getByText('Task 11 in ws 1').first()).toBeVisible();

  requests.length = 0;
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /Other/ }).first().click();
  await expect(page).toHaveURL(/\/other\/board/);
  await expect(page.getByText('Task 31 in ws 2').first()).toBeVisible();
  await expect(page.getByText('Task 11 in ws 1').first()).toBeHidden();
  expect(errors.filter((e) => /Unhandled error/.test(e))).toEqual([]);

  // The switch itself is entirely local: no PUT of the account's current_workspace.
  expect(
    requests.some((r) => r.path === 'v2/user/settings' && r.method === 'PUT'),
  ).toBe(false);
  // Every request after the switch is scoped to the new workspace by header.
  expect(requests.filter((r) => r.path.endsWith('statuses'))[0]?.workspaceId).toBe(2);

  await page.goto('/other/list');
  await page.goto('/other/board');
  await expect(page.getByText('Task 31 in ws 2').first()).toBeVisible();

  expect(
    errors.filter((e) => /parentNode|bum|Unhandled error|runtime-15/.test(e)),
  ).toEqual([]);
});

// The sidebar's "top categories" shortcut (project_categories/children) isn't workspace-scoped
// by header — it's excluded here deliberately, not by omission.
const isWorkspaceScopedPath = (path) =>
  path === 'workspaces/statuses' ||
  path === 'project_categories' ||
  /^workspaces\/\d+\/members$/.test(path) ||
  /^tasks\/status\//.test(path);

function trackRequests(page) {
  const requests = [];
  page.on('request', (req) => {
    const url = new URL(req.url());
    if (!url.pathname.includes('/api/')) return;
    requests.push({
      path: url.pathname.replace(/^.*\/api\//, ''),
      method: req.method(),
      // Raw, no fallback: a missing header must show up as missing, not as workspace 1.
      workspaceId: req.headers()['x-workspace-id'] ?? null,
    });
  });
  return requests;
}

test('two tabs on different workspaces do not affect each other', async ({
  browser,
}) => {
  const context = await browser.newContext();
  await routeApi(context);
  await context.addInitScript(() =>
    localStorage.setItem('token', JSON.stringify({ token: 'fixture' })),
  );
  await context.routeWebSocket(/.*/, () => {});

  const pageA = await context.newPage();
  const pageB = await context.newPage();

  await pageA.goto('/demo/board');
  await expect(pageA.getByText('Task 11 in ws 1').first()).toBeVisible();

  await pageB.goto('/demo/board');
  await expect(pageB.getByText('Task 11 in ws 1').first()).toBeVisible();

  // Switch only B, through the real sidebar UI — same localStorage as A, independent
  // sessionStorage. A must not move, and the switch itself must never PUT.
  const requestsB = trackRequests(pageB);
  await pageB.getByTitle('Switch workspace').first().click();
  await pageB.getByRole('menuitem', { name: /Other/ }).first().click();
  await expect(pageB).toHaveURL(/\/other\/board/);
  await expect(pageB.getByText('Task 31 in ws 2').first()).toBeVisible();
  expect(
    requestsB.some((r) => r.path === 'v2/user/settings' && r.method === 'PUT'),
  ).toBe(false);

  await expect(pageA.getByText('Task 11 in ws 1').first()).toBeVisible();
  await expect(pageA).toHaveURL(/\/demo\/board/);

  // Reload each tab: every workspace-scoped request carries exactly that tab's id.
  const requestsA = trackRequests(pageA);
  await pageA.reload();
  await expect(pageA.getByText('Task 11 in ws 1').first()).toBeVisible();
  const scopedA = requestsA.filter((r) => isWorkspaceScopedPath(r.path));
  expect(scopedA.length).toBeGreaterThan(0);
  expect(scopedA.every((r) => r.workspaceId === '1')).toBe(true);

  requestsB.length = 0;
  await pageB.reload();
  await expect(pageB.getByText('Task 31 in ws 2').first()).toBeVisible();
  const scopedB = requestsB.filter((r) => isWorkspaceScopedPath(r.path));
  expect(scopedB.length).toBeGreaterThan(0);
  expect(scopedB.every((r) => r.workspaceId === '2')).toBe(true);

  // B's switch wrote localStorage's "last used" to 2, shared with A — but A's own
  // sessionStorage (set when it first loaded /demo) still outranks it.
  await pageA.goto('/');
  await expect(pageA).toHaveURL(/\/demo\//);
  await pageB.goto('/');
  await expect(pageB).toHaveURL(/\/other\//);

  await context.close();
});
