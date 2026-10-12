import { buildGraphMap, filterByRange } from './graphMapFixture.mjs';

export const S4B_MODULES = [
  'workflow',
  'board',
  'categories',
  'task.relations',
  'task.files',
  'task.checkpoints',
  'pomodoro',
  'exports',
  'pages',
  'graph',
  'daily_routines',
];

const author = { kind: 'user', id: 1, name: 'Test User' };

export const mockModules = async (page, { graph = true } = {}) => {
  const state = { graph };
  const entries = () =>
    S4B_MODULES.map((key) => ({
      key,
      name: key,
      pack: 'core',
      scope: 'workspace',
      enabled: key === 'graph' ? state.graph : true,
    }));
  const toggles = () =>
    Object.fromEntries(
      entries().map((m) => [
        m.key,
        { key: m.key, name: m.key, group: 'pages', type: 'boolean', enabled: m.enabled },
      ]),
    );
  await page.route('**/api/workspaces/1/modules', (route) =>
    route.fulfill({
      json: { data: { configured: true, can_manage: true, modules: entries() } },
    }),
  );
  await page.route('**/api/**/feature-toggles', async (route) => {
    const request = route.request();
    if (request.url().includes('/user/')) {
      return route.fulfill({
        json: {
          data: {
            default_landing_page: {
              key: 'default_landing_page',
              name: 'Default Landing Page',
              group: 'general',
              type: 'select',
              options: ['list', 'board'],
              value: 'list',
            },
          },
        },
      });
    }
    if (request.method() === 'PUT') {
      const features = request.postDataJSON()?.features ?? {};
      if (features.graph !== undefined) state.graph = features.graph === true;
    }
    return route.fulfill({ json: { data: toggles() } });
  });
  return state;
};

export const mockPages = async (page, calls) => {
  const state = {
    pages: [
      { id: 1, title: 'Handbook', slug: 'handbook', type: 'plain', parent_id: null, version: 1, body: 'Hello' },
    ],
    nextId: 100,
  };
  const summary = (p) => ({
    id: p.id,
    title: p.title,
    slug: p.slug,
    type: p.type,
    parent_id: p.parent_id,
    position: p.id,
    pinned: false,
    updated_at: '2026-10-01T10:00:00Z',
  });
  const full = (p) => ({
    ...summary(p),
    workspace_id: 1,
    body: p.body,
    properties: p.properties ?? {},
    version: p.version,
    author,
    updated_by: author,
    created_at: '2026-10-01T10:00:00Z',
    backlinks: [],
    sections: [],
    files: [],
    following: false,
  });
  await page.route('**/api/pages**', (route) => {
    const request = route.request();
    const method = request.method();
    const path = new URL(request.url()).pathname.replace('/api/pages', '').replace(/^\//, '');
    const json = (data, status = 200) => route.fulfill({ status, json: { data } });
    if (method === 'GET' && (path === 'tree' || path === '')) return json(state.pages.map(summary));
    if (method === 'POST' && path === '') {
      calls.push('POST pages');
      const body = request.postDataJSON();
      const id = state.nextId++;
      const created = {
        id,
        title: body.title ?? 'Untitled',
        slug: `p-${id}`,
        type: body.type ?? 'plain',
        parent_id: body.parent_id ?? null,
        body: body.body ?? '',
        properties: body.properties ?? {},
        version: 1,
      };
      state.pages.push(created);
      return json(full(created), 201);
    }
    const target = state.pages.find((p) => String(p.id) === path.split('/')[0] || p.slug === path.split('/')[0]);
    if (target && method === 'GET') return json(full(target));
    return json([]);
  });
  return state;
};

const routineEntry = {
  task_id: 31,
  title: 'Stretch',
  description: null,
  routine_category: { id: 'health', name: 'Health', color: '#4caf50' },
  date: new Date().toISOString().slice(0, 10),
  time: '08:00',
  duration_min: 15,
  reminder_min: null,
  completed: false,
  status: 'PENDING',
  instance_id: null,
  virtual: false,
  frequency: 'DAILY',
  scheduled_for: null,
};

export const mockRoutines = async (page, calls) => {
  await page.route('**/api/daily-routines/**', (route) => {
    const request = route.request();
    const method = request.method();
    const path = new URL(request.url()).pathname.replace(/^.*\/api\/daily-routines\//, '');
    if (method === 'POST' && (path === 'tasks' || path === 'tasks/quick')) {
      calls.push('POST routines');
      return route.fulfill({ json: { data: { ...routineEntry, task_id: 32, id: 32, title: 'New routine' } } });
    }
    if (method === 'POST' && /complete/.test(path)) {
      calls.push('POST routine complete');
      return route.fulfill({ json: { data: { instance_id: 1, completed: true } } });
    }
    if (path.startsWith('expand/stats')) return route.fulfill({ json: { data: {} } });
    if (path.startsWith('expand')) return route.fulfill({ json: { data: [routineEntry] } });
    if (/^tasks\/\d+$/.test(path)) {
      return route.fulfill({
        json: { data: { id: 31, title: 'Stretch', is_daily_routine: true, settings: [], pattern: { frequency: 'DAILY', interval: 1 } } },
      });
    }
    if (path.startsWith('tasks/count') || path.includes('count')) return route.fulfill({ json: { count: 1 } });
    return route.fulfill({ json: { data: [] } });
  });
};

export const mockGraph = async (page) => {
  const map = buildGraphMap({ tasks: 40, pages: 10, links: 60 });
  await page.route('**/api/graph/map**', (route) => {
    const q = new URL(route.request().url()).searchParams;
    return route.fulfill({ json: { data: filterByRange(map, q.get('from'), q.get('to')) } });
  });
  await page.route('**/api/graph/related**', (route) =>
    route.fulfill({ json: { data: { center: 'task:1', nodes: [], edges: [], truncated: false, caps_hit: {} } } }),
  );
};

export const mockMisc = async (page, calls) => {
  await page.route('**/api/project_categories**', (route) => {
    if (route.request().method() === 'POST') {
      calls.push('POST project_categories');
      return route.fulfill({
        json: { data: { id: 9, title: 'Fixture', code: 'FIX', workspace_id: 1, deleted_at: null, settings: [] } },
      });
    }
    return route.fallback();
  });
  await page.route('**/api/exports/**', (route) => {
    calls.push('GET export');
    return route.fulfill({ body: 'id,title\n1,Original task\n', contentType: 'text/csv' });
  });
};
