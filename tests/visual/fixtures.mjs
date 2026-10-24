// Visual-test data on top of tests/performance/mockApp.mjs. Routes added here are registered after
// mockApp's catch-all, so Playwright matches them first; anything not listed falls back to mockApp.
// Dates are relative to the fixed clock in visualApp.mjs (NOW = 2026-09-15T10:00:00Z).

const DAY = 86_400_000;
const TODAY = Date.UTC(2026, 8, 15);
const dayKey = (back) =>
  new Date(TODAY - back * DAY).toISOString().slice(0, 10);
const iso = (back, hour = 10) =>
  new Date(TODAY - back * DAY + hour * 3_600_000).toISOString();

const user = {
  id: 1,
  name: 'Test User',
  email: 'test@example.test',
  settings: [{ key: 'current_workspace', value: '1' }],
  created_at: '2026-01-01T10:00:00Z',
};
const ann = { id: 2, name: 'Ann Lee', email: 'ann@example.test' };
const bob = { id: 3, name: 'Bob Ray', email: 'bob@example.test' };

const MODULE_KEYS = [
  'board',
  'dashboard',
  'pages',
  'daily_routines',
  'categories',
  'exports',
  'graph',
  'alerts',
  'personas',
  'agent_work',
  'task.files',
  'task.assignees',
  'task.checkpoints',
  'task.countdown',
  'task.relations',
  'pomodoro',
  'workflow',
];

const STATUSES = [
  { id: 1, name: 'Backlog', type: 'default', color: '#94a3b8' },
  { id: 2, name: 'In progress', type: 'active', color: '#3b82f6' },
  { id: 3, name: 'Done', type: 'completed', color: '#22c55e' },
].map((s, i) => ({ ...s, pivot: { order: i + 1, is_active: true } }));

const category = (id, title, code, parent = null) => ({
  id,
  title,
  code,
  slug: code.toLowerCase(),
  project_category_id: parent,
  workspace_id: 1,
  user_id: 1,
  cursor_connected: false,
  deleted_at: null,
  children_count: parent ? 0 : 1,
  tasks_count: 3,
  settings: [],
});
const CATEGORIES = [
  category(10, 'Work', 'WORK'),
  category(11, 'Website', 'WEB', 10),
  category(12, 'Personal', 'HOME'),
];

const DESCRIPTION = [
  '## Goal',
  '',
  'Ship the **new onboarding** flow with `feature flags` and a [spec link](https://example.test).',
  '',
  '- Draft the copy',
  '- Review with design',
  '- [ ] Write e2e tests',
  '',
  '> Keep the old flow behind a toggle.',
  '',
  '```js',
  'const ready = true;',
  '```',
].join('\n');

const task = (id, statusId, title, extra = {}) => ({
  id,
  title,
  description: null,
  description_json: null,
  status: 'created',
  status_id: statusId,
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
  priority: null,
  deleted_at: null,
  created_at: iso(3),
  updated_at: iso(1),
  ...extra,
});

const TASKS = [
  task(1, 1, 'Original task', {
    description: DESCRIPTION,
    priority: 'high',
    project_category_id: 10,
    project_category: CATEGORIES[0],
    assignees: [ann],
    common_time: 5400,
    approximately_time: 14400,
    checkpoints: [
      { id: 1, title: 'Copy approved', done: true, is_done: true },
      { id: 2, title: 'Tests green', done: false, is_done: false },
    ],
  }),
  task(2, 1, 'Fix login redirect on expired session', {
    priority: 'urgent',
    project_category_id: 11,
    project_category: CATEGORIES[1],
  }),
  task(3, 1, 'Write release notes', { priority: 'low' }),
  task(4, 2, 'Refactor the board drag and drop', {
    priority: 'medium',
    assignees: [user, bob],
    common_time: 1800,
  }),
  task(5, 3, 'Update dependencies', { project_category_id: 12 }),
];

const comment = (id, who, message, back) => ({
  id,
  message,
  user: who,
  created_at: iso(back, 9 + id),
  updated_at: iso(back, 9 + id),
  reactions: [],
});
const COMMENTS = [
  comment(1, ann, 'Can we split this into two PRs?', 2),
  comment(
    2,
    user,
    'Sure — copy first, then the tests.\nI will tag you on both.',
    1,
  ),
  comment(3, ann, 'Thanks! 👍', 0),
];

const notification = (id, title, message, read, back) => ({
  id,
  user_id: 1,
  workspace_id: 1,
  activity_id: null,
  type: 'task_updated',
  title,
  message,
  data: null,
  link: null,
  read_at: read ? iso(back, 12) : null,
  created_at: iso(back),
  updated_at: iso(back),
});
const NOTIFICATIONS = [
  notification(
    1,
    'Ann Lee commented',
    'Can we split this into two PRs?',
    false,
    0,
  ),
  notification(
    2,
    'Task assigned to you',
    'Refactor the board drag and drop',
    false,
    1,
  ),
  notification(3, 'Status changed', 'Update dependencies → Done', true, 2),
];

const pageRow = (id, title, slug, parent_id = null, extra = {}) => ({
  id,
  title,
  slug,
  type: 'plain',
  parent_id,
  position: id,
  pinned: false,
  workspace_id: 1,
  updated_at: iso(id),
  ...extra,
});
const author = { kind: 'user', id: 1, name: 'Test User' };
const PAGE_BODY = [
  '# Release checklist',
  '',
  'Everything we check **before** a tag. See `CHANGELOG.md` and [the board](https://example.test).',
  '',
  '## Steps',
  '',
  '1. Bump the version',
  '2. Run the suites',
  '3. Tag and push',
  '',
  '- [x] Changelog written',
  '- [ ] Desktop build signed',
  '',
  '> Never tag on a Friday.',
  '',
  '```bash',
  'npm run build',
  '```',
  '',
  '| Check | Owner |',
  '| --- | --- |',
  '| Jest | CI |',
  '| E2E | QA |',
].join('\n');
const PAGES = [
  pageRow(1, 'Release checklist', 'release-checklist', null, { pinned: true }),
  pageRow(2, 'Meeting notes', 'meeting-notes'),
  pageRow(3, 'Week 37', 'week-37', 2),
  pageRow(4, 'Ideas', 'ideas'),
];
const fullPage = (row) => ({
  ...row,
  body: row.id === 1 ? PAGE_BODY : 'Short page.',
  properties: {},
  version: 3,
  author,
  updated_by: author,
  created_at: iso(10),
  backlinks: [],
  sections: [],
  files: [],
  following: false,
});

const routine = (id, title, time, cat, color, completed = false) => ({
  task_id: id,
  title,
  description: null,
  routine_category: {
    id: cat,
    name: cat[0].toUpperCase() + cat.slice(1),
    color,
  },
  date: dayKey(0),
  time,
  duration_min: 20,
  reminder_min: 15,
  completed,
  status: completed ? 'completed' : 'pending',
  instance_id: completed ? id * 10 : null,
  virtual: !completed,
  frequency: 'DAILY',
  scheduled_for: null,
});
const ROUTINES = [
  routine(31, 'Stretch', '08:15', 'health', '#22c55e', true),
  routine(32, 'Inbox zero', '09:30', 'work', '#3b82f6'),
  routine(33, 'Read 20 pages', '21:00', 'personal', '#a855f7'),
];

const heatmap = () => ({
  contributions: Array.from({ length: 365 }, (_, i) => {
    const back = 364 - i;
    const count =
      back < 7 ? 3 : back % 5 === 0 ? 0 : back < 120 ? (back % 4) + 1 : 0;
    return { date: dayKey(back), count, level: Math.min(count, 4) };
  }),
  total_contributions: 240,
  streak: { current: 12, longest: 30 },
  year: 2026,
  weeks: 53,
});
const member = (m, activeDays, days, i) => ({
  ...m,
  role: i ? 'member' : 'owner',
  is_online: !i,
  last_activity_at: iso(0, 9),
  last_task: null,
  current_task: null,
  tracked_seconds: 1800 * (i + 1),
  done_count: i + 2,
  active_tasks: 2,
  completed_tasks: i + 1,
  comments_count: i,
  activity_count: 3,
  daily_activity: Array.from({ length: days }, (_, d) => ({
    date: dayKey(days - 1 - d),
    count: days - 1 - d < activeDays ? 2 : 0,
  })),
  streak: activeDays,
});
const teamActivity = (window) => {
  const days =
    window === '30d' || window === 'all' ? 30 : window === '7d' ? 7 : 1;
  return {
    members: [member(user, 5, days, 0), member(ann, 2, days, 1)],
    total_members: 2,
    online_members: 1,
    active_today: 2,
    active_timers: 0,
    tracked_seconds: 5400,
    done_count: 7,
    window,
  };
};
const timeDaily = (from, to) => {
  const days = [];
  for (let t = Date.parse(from); t <= Date.parse(to); t += DAY) {
    const seconds = 1800 + ((t / DAY) % 3) * 900;
    days.push({
      date: new Date(t).toISOString().slice(0, 10),
      total_s: seconds,
      items: [
        {
          task: { id: 1, key: 'WORK-1', title: 'Original task', url: null },
          status: null,
          category: { id: 10, code: 'WORK', name: 'Work' },
          seconds,
          users: [{ id: 1, name: 'Test User', seconds }],
        },
      ],
    });
  }
  return { from, to, tz: 'UTC', days };
};

const page1 = (data) => ({
  data,
  meta: {
    current_page: 1,
    per_page: 10,
    total: data.length,
    last_page: 1,
    from: data.length ? 1 : null,
    to: data.length || null,
  },
});

/**
 * Installs the visual data set. `empty: true` answers tasks, pages and notifications with nothing,
 * for the empty states.
 */
export async function withData(page, { empty = false } = {}) {
  const tasks = empty ? [] : TASKS;
  const json = (route, data) => route.fulfill({ json: { data } });
  const api = (pattern, handler) =>
    page.route(
      (url) =>
        url.pathname.startsWith('/api/') && pattern(url.pathname.slice(5), url),
      handler,
    );

  await api(
    (p) => p === 'workspaces',
    (r) =>
      json(r, [
        { id: 1, name: 'Demo', code: 'demo', user_id: 1, is_default: true },
        { id: 2, name: 'Side project', code: 'side', user_id: 1 },
      ]),
  );
  await api(
    (p) => p.endsWith('/modules'),
    (r) =>
      json(r, { modules: MODULE_KEYS.map((key) => ({ key, enabled: true })) }),
  );
  await api(
    (p) => p.endsWith('statuses'),
    (r) => json(r, STATUSES),
  );
  await api(
    (p) => /^workspaces\/\d+\/members$/.test(p),
    (r) => json(r, [user, ann, bob]),
  );
  await api(
    (p) => p.startsWith('tasks/status/'),
    (r) => {
      const id = Number(new URL(r.request().url()).pathname.split('/').pop());
      return json(
        r,
        tasks.filter((t) => t.status_id === id),
      );
    },
  );
  await api(
    (p) => p === 'tasks/current' || p === 'tasks/' || p === 'tasks',
    (r) =>
      r.request().method() === 'GET'
        ? r.fulfill({ json: page1(tasks) })
        : r.fallback(),
  );
  await api(
    (p) => /^tasks\/\d+$/.test(p),
    (r) => {
      const id = Number(new URL(r.request().url()).pathname.split('/').pop());
      const found = TASKS.find((t) => t.id === id) ?? TASKS[0];
      if (r.request().method() === 'GET') return json(r, found);
      return json(r, { ...found, ...r.request().postDataJSON() });
    },
  );
  await api(
    (p) => /^tasks\/\d+\/comments\/?$/.test(p),
    (r) =>
      r.request().method() === 'GET' ? json(r, COMMENTS) : json(r, COMMENTS[0]),
  );
  await api(
    (p) => p.startsWith('project_categories'),
    (r) => {
      const p = new URL(r.request().url()).pathname;
      if (p.endsWith('/settings')) return json(r, []);
      if (/project_categories\/\d+$/.test(p))
        return json(
          r,
          CATEGORIES.find((c) => p.endsWith(`/${c.id}`)) ?? CATEGORIES[0],
        );
      return json(r, CATEGORIES);
    },
  );
  await api(
    (p) => p === 'notification-settings',
    (r) => {
      const types = ['task_assigned', 'task_status_changed', 'comment_created'];
      return r.request().method() === 'GET'
        ? r.fulfill({
            json: {
              settings: {
                web_enabled: true,
                web_types: types,
                telegram_enabled: false,
                telegram_types: [],
                email_enabled: true,
                email_types: ['task_assigned'],
                agent_push_enabled: true,
                agent_telegram_enabled: false,
              },
              available_types: types,
            },
          })
        : r.fallback();
    },
  );
  await api(
    (p) => p.startsWith('notifications'),
    (r) => {
      const p = new URL(r.request().url()).pathname;
      const items = empty ? [] : NOTIFICATIONS;
      if (p.endsWith('/unread-count'))
        return r.fulfill({
          json: { count: items.filter((n) => !n.read_at).length },
        });
      return r.fulfill({
        json: {
          data: items,
          current_page: 1,
          last_page: 1,
          per_page: 20,
          total: items.length,
        },
      });
    },
  );
  await api(
    (p) => p === 'pages' || p.startsWith('pages/'),
    (r) => {
      const rest = new URL(r.request().url()).pathname.replace(
        /^\/api\/pages\/?/,
        '',
      );
      const rows = empty ? [] : PAGES;
      if (rest === '' || rest === 'tree') return json(r, rows);
      const row = PAGES.find((x) => x.slug === rest || String(x.id) === rest);
      if (row && r.request().method() === 'GET') return json(r, fullPage(row));
      return json(r, []);
    },
  );
  await api(
    (p) => p.startsWith('daily-routines/'),
    (r) => {
      const p = new URL(r.request().url()).pathname;
      if (p.includes('expand/stats')) return json(r, {});
      if (p.includes('/expand')) return json(r, ROUTINES);
      if (p.endsWith('/count'))
        return r.fulfill({
          json: {
            count: p.includes('completed') ? 1 : p.includes('archived') ? 0 : 3,
          },
        });
      return json(r, []);
    },
  );
  await api(
    (p) => p.includes('/dashboard/'),
    (r) => {
      const url = new URL(r.request().url());
      if (url.pathname.endsWith('heatmap')) return json(r, heatmap());
      if (url.pathname.endsWith('team-activity'))
        return json(r, teamActivity(url.searchParams.get('window')));
      if (url.pathname.endsWith('activities'))
        return r.fulfill({
          json: {
            data: [],
            meta: { current_page: 1, per_page: 20, total: 0, last_page: 1 },
            links: {},
          },
        });
      return json(r, {});
    },
  );
  await api(
    (p) => /time\/daily$/.test(p),
    (r) => {
      const url = new URL(r.request().url());
      return json(
        r,
        timeDaily(url.searchParams.get('from'), url.searchParams.get('to')),
      );
    },
  );
}
