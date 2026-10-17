export const SHOTS = process.env.TOURS_SHOTS ?? 'test-results/tours';

export const SIZES = [
  { name: 'desktop', width: 1280, height: 800 },
  { name: 'phone', width: 390, height: 844 },
];
export const WIDE = { name: '1440', width: 1440, height: 900 };
export const SCHEMES = ['light', 'dark'];
export const AUTO = { tips_mode: 'auto' };

const freshState = () => ({
  intent: null,
  tips_mode: 'auto',
  hub_hidden: false,
  checklist_hidden: false,
  tours_generation: 0,
  tours: {},
  facts: {},
});

export const mockOnboarding = async (page, seed = {}) => {
  const state = { ...freshState(), ...seed };
  const writes = [];
  await page.route('**/api/onboarding**', async (route) => {
    const request = route.request();
    const method = request.method();
    const path = new URL(request.url()).pathname.replace(/^.*\/api\//, '');
    const body =
      method === 'GET' || method === 'DELETE' ? null : request.postDataJSON();
    if (method !== 'GET') writes.push({ method, path, body });
    if (path === 'onboarding/preferences' && method === 'PUT') {
      for (const key of ['tips_mode', 'hub_hidden', 'checklist_hidden']) {
        if (body?.[key] !== undefined) state[key] = body[key];
      }
    } else if (path.startsWith('onboarding/tours/') && method === 'PUT') {
      const id = decodeURIComponent(path.slice('onboarding/tours/'.length));
      if ((body.generation ?? 0) >= state.tours_generation) {
        state.tours[id] = {
          status: body.status,
          step_id: body.step_id ?? null,
          version: body.version ?? 1,
          generation: body.generation ?? 0,
        };
      }
    } else if (path === 'onboarding/tours' && method === 'DELETE') {
      state.tours = {};
      state.tours_generation += 1;
    } else if (path === 'onboarding/intent' && method === 'POST') {
      state.intent = body.intent;
    }
    const data = path === 'onboarding/facts' ? {} : state;
    await route.fulfill({ json: { data } });
  });
  return { state, writes };
};

const baseTask = {
  description: null,
  description_json: null,
  status: 'created',
  status_id: 1,
  workspace_id: 1,
  user_id: 1,
  assignees: [],
  settings: [],
  checkpoints: [],
  common_time: 0,
  start_time: 0,
  approximately_time: 0,
  project_category_id: null,
  created_at: '2026-10-12T10:00:00Z',
  updated_at: '2026-10-12T10:00:00Z',
};

export const mockTasks = async (page) => {
  const created = [];
  await page.route('**/api/tasks', async (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    const payload = route.request().postDataJSON();
    const task = {
      ...baseTask,
      id: 50 + created.length,
      title: payload.title ?? 'New task',
    };
    created.push(task);
    await route.fulfill({ json: { data: task } });
  });
  await page.route('**/api/tasks/current**', async (route) => {
    const rows = [...created, { ...baseTask, id: 1, title: 'Original task' }];
    await route.fulfill({
      json: {
        data: rows,
        meta: {
          current_page: 1,
          per_page: 10,
          total: rows.length,
          last_page: 1,
          from: 1,
          to: rows.length,
        },
      },
    });
  });
  return created;
};

export const setScheme = async (page, scheme) => {
  await page.emulateMedia({ colorScheme: scheme });
  await page.evaluate(
    (dark) => document.documentElement.classList.toggle('dark', dark),
    scheme === 'dark',
  );
  await page.waitForTimeout(300);
};

export const shoot = async (page, name, size, scheme) => {
  await setScheme(page, scheme);
  await page.screenshot({
    path: `${SHOTS}/${name}-${size.name}-${scheme}.png`,
  });
};
