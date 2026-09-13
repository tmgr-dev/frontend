export async function mockApp(
  page,
  {
    failCategories = false,
    replaceIdentity = false,
    taskCount = 1,
    multiStatus = false,
    blockTask = false,
  } = {},
) {
  await page.addInitScript(() =>
    localStorage.setItem('token', JSON.stringify({ token: 'fixture' })),
  );
  await page.routeWebSocket(/.*/, () => {});
  const user = {
    id: 1,
    name: 'Test User',
    email: 'test@example.test',
    settings: [{ key: 'current_workspace', value: '1' }],
  };
  const task = {
    id: 1,
    title: 'Original task',
    description: null,
    description_json: blockTask
      ? {
          time: 1,
          version: '2.30.6',
          blocks: [
            {
              id: 'initial',
              type: 'paragraph',
              data: { text: 'Initial block' },
            },
          ],
        }
      : null,
    status: 'created',
    status_id: 1,
    workspace_id: 1,
    user_id: 1,
    user,
    assignees: [],
    settings: [],
    checkpoints: [],
    common_time: 0,
    start_time: 0,
    approximately_time: 0,
    project_category_id: null,
    created_at: '2026-09-13T10:00:00Z',
    updated_at: '2026-09-13T10:00:00Z',
  };
  let release;
  let requests = 0;
  let boardRequests = 0;
  const savedTasks = [];
  let categoriesFail = failCategories;
  await page.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/', '');
    let data = [];
    if (path === 'project_categories' && categoriesFail) {
      await route.fulfill({
        status: 500,
        json: { message: 'Fixture failure' },
      });
      return;
    }
    if (/^tasks\/\d+$/.test(path)) {
      const id = Number(path.split('/')[1]);
      data = { ...task, id, title: id === 1 ? 'Original task' : 'Other task' };
      if (route.request().method() === 'PUT') {
        const payload = route.request().postDataJSON();
        savedTasks.push({ id, payload });
        data = { ...data, ...payload };
      }
    } else if (path === 'user') data = user;
    else if (path === 'workspaces')
      data = [
        { id: 1, name: 'Demo', code: 'demo', user_id: 1, is_default: true },
      ];
    else if (path.endsWith('feature-toggles'))
      data = path.startsWith('user')
        ? { default_landing_page: { value: 'list' } }
        : { board: { enabled: true } };
    else if (path.endsWith('statuses'))
      data = [
        {
          id: 1,
          name: 'Backlog',
          type: 'default',
          color: '#888888',
          pivot: { order: 1, is_active: true },
        },
      ];
    else if (path.endsWith('members')) data = [user];
    else if (path.startsWith('tasks/status/')) {
      boardRequests++;
      data = Array.from({ length: taskCount }, (_, i) => ({
        ...task,
        id: i + 1,
        title: i ? `Fixture task ${i + 1}` : task.title,
      }));
    } else if (path === 'tasks/current') {
      requests++;
      const refresh = !!url.searchParams.get('search');
      if (refresh)
        await new Promise((resolve) => {
          release = resolve;
        });
      await route.fulfill({
        json: {
          data: [
            {
              ...task,
              id:
                replaceIdentity && url.searchParams.get('page') === '2'
                  ? 99
                  : 1,
              title: refresh
                ? 'Updated task'
                : url.searchParams.get('page') === '2'
                ? 'Second page task'
                : task.title,
            },
          ],
          meta: {
            current_page: Number(url.searchParams.get('page') || 1),
            per_page: 10,
            total: 20,
            last_page: 2,
            from: 1,
            to: 10,
          },
        },
      });
      return;
    }
    if (multiStatus && path.endsWith('statuses'))
      data.push({
        id: 2,
        name: 'In progress',
        type: 'active',
        color: '#888888',
        pivot: { order: 2, is_active: true },
      });
    if (multiStatus && path === 'tasks/status/2') data = [];
    await route.fulfill({ json: { data } });
  });
  return {
    recover: () => {
      categoriesFail = false;
    },
    ready: () => !!release,
    release: () => release(),
    requests: () => requests,
    boardRequests: () => boardRequests,
    savedTasks,
  };
}
