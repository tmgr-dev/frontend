export async function mockApp(
  page,
  {
    failCategories = false,
    replaceIdentity = false,
    taskCount = 1,
    multiStatus = false,
    blockTask = false,
    account = true,
    apiRequests = [],
    loginAs = null,
    authHeaders = [],
    consent = true,
    onboarding = null,
    userExtra = {},
    taskSettings = [],
  } = {},
) {
  if (consent)
    await page.addInitScript(() => {
      localStorage.setItem(
        'tmgr:consent:v1',
        JSON.stringify({ analytics: 'denied', updatedAt: null }),
      );
      localStorage.setItem(
        'tmgr:telemetry:consent',
        JSON.stringify({ on: false, generation: 1 }),
      );
    });
  await page.routeWebSocket(/.*/, () => {});
  if (!account && !loginAs) {
    await page.route('**/api/**', async (route) => {
      apiRequests.push(
        `${route.request().method()} ${
          new URL(route.request().url()).pathname
        }`,
      );
      await route.fulfill({ status: 500, json: { message: 'no account' } });
    });
    return;
  }
  if (account)
    await page.addInitScript(() =>
      localStorage.setItem('token', JSON.stringify({ token: 'fixture' })),
    );
  const user = {
    id: 1,
    name: 'Test User',
    email: 'test@example.test',
    settings: [{ key: 'current_workspace', value: '1' }],
    created_at: '2026-01-01T10:00:00Z',
    ...userExtra,
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
    settings: taskSettings,
    checkpoints: [],
    common_time: 0,
    start_time: 0,
    approximately_time: 0,
    project_category_id: null,
    created_at: '2026-09-13T10:00:00Z',
    updated_at: '2026-09-13T10:00:00Z',
  };
  const onboardingFixture = onboarding
    ? {
        state: {
          intent: null,
          invited: false,
          tips_mode: 'hub_only',
          hub_hidden: false,
          checklist_hidden: false,
          tours_generation: 0,
          tours: {},
          facts: {},
          ...onboarding,
        },
        writes: [],
        factsRequests: [],
        starterTasks: onboarding.starterTasks ?? 0,
      }
    : null;
  if (onboardingFixture) delete onboardingFixture.state.starterTasks;
  let release;
  let requests = 0;
  let boardRequests = 0;
  const savedTasks = [];
  let categoriesFail = failCategories;
  await page.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/', '');
    let data = [];
    if (onboardingFixture && path.startsWith('onboarding')) {
      const fx = onboardingFixture;
      const method = route.request().method();
      const body =
        method === 'GET' || method === 'DELETE'
          ? null
          : route.request().postDataJSON();
      if (method !== 'GET') fx.writes.push({ method, path, body });
      let out = fx.state;
      if (path === 'onboarding/facts') {
        const keys = (url.searchParams.get('keys') ?? '')
          .split(',')
          .filter(Boolean);
        fx.factsRequests.push({
          keys,
          taskId: url.searchParams.get('task_id'),
        });
        out = Object.fromEntries(
          keys.map((k) => [k, fx.state.facts[k] ?? false]),
        );
      } else if (path === 'onboarding/intent' && method === 'POST') {
        fx.state.intent = body.intent;
        fx.state.intent_at = new Date().toISOString();
        const created = body.intent === 'human' && body.seed !== false ? 3 : 0;
        fx.starterTasks = created;
        out = { ...fx.state, starter_tasks_created: created };
      } else if (path === 'onboarding/starter-tasks' && method === 'DELETE') {
        out = { removed: fx.starterTasks };
        fx.starterTasks = 0;
      } else if (path === 'onboarding/preferences' && method === 'PUT') {
        for (const key of ['tips_mode', 'hub_hidden', 'checklist_hidden'])
          if (body?.[key] !== undefined) fx.state[key] = body[key];
      } else if (path.startsWith('onboarding/tours/') && method === 'PUT') {
        fx.state.tours[
          decodeURIComponent(path.slice('onboarding/tours/'.length))
        ] = {
          status: body.status,
          step_id: body.step_id ?? null,
          version: body.version ?? 1,
          generation: body.generation ?? 0,
        };
      } else if (path === 'onboarding/tours' && method === 'DELETE') {
        fx.state.tours = {};
        fx.state.tours_generation += 1;
      }
      await route.fulfill({ json: { data: out } });
      return;
    }
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
        ? {
            default_landing_page: {
              key: 'default_landing_page',
              name: 'Default Landing Page',
              group: 'general',
              type: 'select',
              options: ['list', 'board', 'dashboard', 'daily_routines'],
              value: 'list',
            },
          }
        : {
            board: {
              key: 'board',
              name: 'Board View',
              group: 'pages',
              type: 'boolean',
              enabled: true,
            },
          };
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
  if (!account) {
    let anonymous = true;
    await page.route('**/api/**', async (route) => {
      const request = route.request();
      const path = new URL(request.url()).pathname;
      if (
        anonymous &&
        request.method() === 'POST' &&
        path.endsWith('/auth/login')
      ) {
        apiRequests.push(`POST ${path}`);
        anonymous = false;
        await route.fulfill({ json: { data: { token: loginAs } } });
        return;
      }
      if (anonymous) {
        apiRequests.push(`${request.method()} ${path}`);
        await route.fulfill({ status: 500, json: { message: 'no account' } });
        return;
      }
      authHeaders.push(request.headers().authorization);
      await route.fallback();
    });
  }
  return {
    recover: () => {
      categoriesFail = false;
    },
    ready: () => !!release,
    release: () => release(),
    requests: () => requests,
    boardRequests: () => boardRequests,
    savedTasks,
    onboarding: onboardingFixture,
  };
}
