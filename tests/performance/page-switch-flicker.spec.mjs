import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

// TM-682: Dashboard <-> List switches must not flash skeletons once a page has shown data.
const DELAY = 1500;
const DAY = 86_400_000;
const todayUtc = () => {
  const now = new Date();
  return Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
};
const dayKey = (back) =>
  new Date(todayUtc() - back * DAY).toISOString().slice(0, 10);
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const heatmap = () => ({
  contributions: Array.from({ length: 60 }, (_, i) => ({
    date: dayKey(59 - i),
    count: 1,
    level: 1,
  })),
  total_contributions: 60,
  streak: { current: 60, longest: 60 },
  year: new Date().getUTCFullYear(),
  weeks: 53,
});

const teamActivity = (window, done) => ({
  members: [
    {
      id: 1,
      name: 'Ann Lee',
      email: 'ann@example.test',
      role: 'member',
      is_online: true,
      last_activity_at: new Date().toISOString(),
      last_task: null,
      current_task: null,
      tracked_seconds: 1800,
      done_count: done,
      active_tasks: 1,
      completed_tasks: 1,
      comments_count: 0,
      activity_count: 1,
      daily_activity: [{ date: dayKey(0), count: 1 }],
      streak: 1,
    },
  ],
  total_members: 1,
  online_members: 1,
  active_today: 1,
  active_timers: 0,
  tracked_seconds: 1800,
  done_count: done,
  window,
});

const timeDaily = (from, to) => {
  const days = [];
  for (let t = Date.parse(from); t <= Date.parse(to); t += DAY)
    days.push({
      date: new Date(t).toISOString().slice(0, 10),
      total_s: 1800,
      items: [
        {
          task: { id: 1, key: 'DEV-1', title: 'Task', url: null },
          status: null,
          category: { id: 1, code: 'DEV', name: 'Development' },
          seconds: 1800,
          users: [{ id: 1, name: 'Ann Lee', seconds: 1800 }],
        },
      ],
    });
  return { from, to, tz: 'UTC', days };
};

const setup = async (page) => {
  const fixture = {
    done: 5,
    title: 'Original task',
    failTeam: false,
    failList: false,
    emptyList: false,
  };
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page);
  await page.route('**/api/workspaces/1/modules', (r) =>
    r.fulfill({
      json: {
        data: {
          modules: ['board', 'dashboard', 'exports'].map((key) => ({
            key,
            enabled: true,
          })),
        },
      },
    }),
  );
  await page.route(
    (u) => u.pathname.startsWith('/api/') && u.pathname.includes('/dashboard/'),
    async (r) => {
      await wait(DELAY);
      const url = new URL(r.request().url());
      const p = url.pathname;
      if (p.endsWith('heatmap'))
        return r.fulfill({ json: { data: heatmap() } });
      if (p.endsWith('team-activity')) {
        if (fixture.failTeam)
          return r.fulfill({ status: 400, json: { message: 'Team down' } });
        return r.fulfill({
          json: {
            data: teamActivity(url.searchParams.get('window'), fixture.done),
          },
        });
      }
      if (p.endsWith('activities'))
        return r.fulfill({
          json: {
            data: [
              {
                id: 1,
                type: 'task_created',
                description: 'Created a fixture task',
                user: { id: 1, name: 'Ann Lee' },
                subject_type: 'Task',
                subject_id: 1,
                created_at: new Date().toISOString(),
              },
            ],
            meta: { current_page: 1, per_page: 20, total: 1, last_page: 1 },
            links: {},
          },
        });
      return r.fulfill({ json: { data: { total_tasks: 1 } } });
    },
  );
  await page.route('**/api/workspaces/1/time/daily**', async (r) => {
    await wait(DELAY);
    const url = new URL(r.request().url());
    return r.fulfill({
      json: {
        data: timeDaily(
          url.searchParams.get('from'),
          url.searchParams.get('to'),
        ),
      },
    });
  });
  await page.route('**/api/tasks/current**', async (r) => {
    await wait(DELAY);
    if (fixture.failList)
      return r.fulfill({ status: 400, json: { message: 'List down' } });
    return r.fulfill({
      json: {
        data: fixture.emptyList
          ? []
          : [
              {
                id: 1,
                title: fixture.title,
                description: null,
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
                created_at: '2026-09-13T10:00:00Z',
                updated_at: '2026-09-13T10:00:00Z',
              },
            ],
        meta: {
          current_page: 1,
          per_page: 10,
          total: 1,
          last_page: 1,
          from: 1,
          to: 1,
        },
      },
    });
  });
  return fixture;
};

// Records every skeleton / loading placeholder that enters the page content, however briefly.
function installFlashWatcher() {
  window.__flashes?.observer.disconnect();
  const found = [];
  const SKELETON = '.animate-pulse.bg-muted, [aria-label="Loading dashboard"]';
  const check = (node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      if (node.data.includes('Refreshing...')) found.push('Refreshing...');
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const hits = [
      ...(node.matches(SKELETON) ? [node] : []),
      ...node.querySelectorAll(SKELETON),
    ].filter((el) => el.closest('main'));
    for (const el of hits) found.push(el.outerHTML.slice(0, 120));
    if (node.closest('main') && node.textContent.includes('Refreshing...'))
      found.push('Refreshing...');
  };
  const observer = new MutationObserver((mutations) => {
    for (const m of mutations) {
      if (m.type !== 'childList') check(m.target);
      else {
        m.addedNodes.forEach(check);
        m.removedNodes.forEach((node) => {
          if (node.textContent.includes('Create your first task'))
            found.push('empty state removed');
        });
      }
    }
  });
  observer.observe(document, {
    subtree: true,
    childList: true,
    characterData: true,
    attributes: true,
    attributeFilter: ['class'],
  });
  window.__flashes = { observer, found };
}
const watchFlashes = (page) => page.evaluate(installFlashWatcher);
const flashes = (page) => page.evaluate(() => window.__flashes.found);

const nav = (page, name) =>
  page
    .locator('[data-sidebar="sidebar"]')
    .getByRole('link', { name, exact: true })
    .click();

const kpi = (page, key) => page.locator(`[data-kpi="${key}"]`);
const activity = (page) => page.getByRole('button', { name: /Created a task/ });
const row = (page) => page.locator('[data-task-id="1"]');
// Shorter than the API delay: content shown this fast came from the last snapshot.
const SEEDED = { timeout: DELAY - 500 };

test('return visits to Dashboard and List render last data without skeletons', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const fixture = await setup(page);
  await page.addInitScript(installFlashWatcher);

  await page.goto('/demo/dashboard');
  await expect(kpi(page, 'completed')).toContainText('5');
  await expect(kpi(page, 'tracked')).toContainText('3h 30m');
  await expect(activity(page)).toBeVisible();
  await expect(page.locator('main .animate-pulse.bg-muted')).toHaveCount(0);
  expect((await flashes(page)).length).toBeGreaterThan(0);

  await watchFlashes(page);
  await nav(page, 'List');
  await expect(row(page)).toContainText('Original task');
  expect((await flashes(page)).length).toBeGreaterThan(0);

  fixture.done = 9;
  await watchFlashes(page);
  await nav(page, 'Dashboard');
  await expect(kpi(page, 'completed')).toContainText('5', SEEDED);
  await expect(kpi(page, 'tracked')).toContainText('3h 30m', SEEDED);
  await expect(activity(page)).toBeVisible(SEEDED);
  await expect(
    page.getByRole('button', { name: /dashboard data/ }),
  ).toContainText('Refresh');
  await expect(kpi(page, 'completed')).toContainText('9');
  await expect(page.locator('main .animate-pulse.bg-muted')).toHaveCount(0);
  await wait(DELAY);
  expect(await flashes(page)).toEqual([]);

  fixture.title = 'Renamed task';
  await watchFlashes(page);
  await nav(page, 'List');
  await expect(row(page)).toContainText('Original task', SEEDED);
  await expect(row(page)).toContainText('Renamed task');
  await wait(DELAY);
  expect(await flashes(page)).toEqual([]);

  await watchFlashes(page);
  await nav(page, 'Dashboard');
  await expect(kpi(page, 'completed')).toContainText('9', SEEDED);
  await page.getByRole('button', { name: /dashboard data/ }).click();
  await expect(kpi(page, 'completed')).toContainText('9');
  await wait(DELAY * 2);
  expect(await flashes(page)).toEqual([]);
});

test('a failed refresh on a return visit keeps the last data and shows the error', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const fixture = await setup(page);

  await page.goto('/demo/dashboard');
  await expect(kpi(page, 'completed')).toContainText('5');
  await expect(page.getByText('Ann Lee').first()).toBeVisible();
  await nav(page, 'List');
  await expect(row(page)).toContainText('Original task');

  fixture.failTeam = true;
  await nav(page, 'Dashboard');
  await expect(kpi(page, 'completed')).toContainText('5', SEEDED);
  await expect(page.getByRole('alert')).toContainText('Dashboard Error', {
    timeout: 15_000,
  });
  await expect(kpi(page, 'completed')).toContainText('5');
  await expect(page.getByText('Ann Lee').first()).toBeVisible();
});

test('a task deleted elsewhere does not ghost into the list snapshot', async ({
  page,
}) => {
  const fixture = await setup(page);
  await page.goto('/demo/list');
  await expect(row(page)).toContainText('Original task');
  await nav(page, 'Dashboard');
  await expect(kpi(page, 'completed')).toContainText('5');

  await page.evaluate(async () => {
    const { requestCache } = await import('/src/utils/requestCache.ts');
    requestCache.invalidate(/^tasks-status-/);
  });
  fixture.title = 'Fresh task';
  await watchFlashes(page);
  await nav(page, 'List');
  await expect(row(page)).toContainText('Fresh task');
  expect((await flashes(page)).length).toBeGreaterThan(0);
});

test('a failed list refresh on a return visit keeps the rows and shows the error', async ({
  page,
}) => {
  const fixture = await setup(page);
  await page.goto('/demo/list');
  await expect(row(page)).toContainText('Original task');
  await nav(page, 'Dashboard');
  await expect(kpi(page, 'completed')).toContainText('5');

  fixture.failList = true;
  await nav(page, 'List');
  await expect(row(page)).toContainText('Original task', SEEDED);
  await expect(
    page.getByText('Could not load tasks. Please try again.'),
  ).toBeVisible();
  await expect(row(page)).toContainText('Original task');
});

test('an empty list keeps its empty state on a return visit', async ({
  page,
}) => {
  const fixture = await setup(page);
  fixture.emptyList = true;
  const empty = page.getByText('Create your first task to get started');
  await page.goto('/demo/list');
  await expect(empty).toBeVisible();
  await nav(page, 'Dashboard');
  await expect(kpi(page, 'completed')).toContainText('5');

  await watchFlashes(page);
  await nav(page, 'List');
  await expect(empty).toBeVisible(SEEDED);
  await wait(DELAY);
  await expect(empty).toBeVisible();
  expect(await flashes(page)).toEqual([]);
});

test('a task deleted through the delete action never shows on the list again', async ({
  page,
}) => {
  const fixture = await setup(page);
  await page.route('**/api/tasks/1', (r) => {
    if (r.request().method() !== 'DELETE') return r.fallback();
    fixture.emptyList = true;
    return r.fulfill({
      json: { data: { deleted_at: '2026-10-21T10:00:00Z' } },
    });
  });
  await page.goto('/demo/list');
  await expect(row(page)).toContainText('Original task');
  await nav(page, 'Dashboard');
  await expect(kpi(page, 'completed')).toContainText('5');

  await page.evaluate(async () => {
    const { deleteTask } = await import('/src/actions/tmgr/tasks.ts');
    await deleteTask(1);
    window.__rowSeen = false;
    const tick = () => {
      if (document.querySelector('[data-task-id="1"]')) window.__rowSeen = true;
      requestAnimationFrame(tick);
    };
    tick();
  });
  await nav(page, 'List');
  await expect(
    page.getByText('Create your first task to get started'),
  ).toBeVisible();
  expect(await page.evaluate(() => window.__rowSeen)).toBe(false);
});

test('logout drops the page snapshots', async ({ page }) => {
  await setup(page);
  await page.goto('/demo/dashboard');
  await expect(kpi(page, 'completed')).toContainText('5');
  await nav(page, 'List');
  await expect(row(page)).toContainText('Original task');
  await nav(page, 'Dashboard');
  await expect(kpi(page, 'completed')).toContainText('5');

  const snapshots = await page.evaluate(async () => {
    const { requestCache } = await import('/src/utils/requestCache.ts');
    const { default: store } = await import('/src/store/index.js');
    const keys = () =>
      [...requestCache['cache'].keys()].filter((k) => k.includes('snapshot'));
    const before = keys().length;
    await store.dispatch('logout');
    return { before, after: keys() };
  });
  expect(snapshots.before).toBeGreaterThan(0);
  expect(snapshots.after).toEqual([]);
});

test('clearing an empty search never offers "Create your first task" while reloading', async ({
  page,
}) => {
  const fixture = await setup(page);
  await page.goto('/demo/list');
  await expect(row(page)).toContainText('Original task');

  fixture.emptyList = true;
  await page.getByPlaceholder('search task').fill('zzz');
  await expect(page.getByText('No tasks match your filters')).toBeVisible({
    timeout: DELAY * 3,
  });
  fixture.emptyList = false;
  await page.evaluate(() => {
    window.__firstTaskSeen = false;
    const tick = () => {
      if (document.body.textContent.includes('Create your first task'))
        window.__firstTaskSeen = true;
      requestAnimationFrame(tick);
    };
    tick();
  });
  await page.getByRole('button', { name: 'Clear filters' }).click();
  await expect(row(page)).toContainText('Original task', {
    timeout: DELAY * 3,
  });
  expect(await page.evaluate(() => window.__firstTaskSeen)).toBe(false);
});

test('a task edited in place on the list is shown edited, without skeletons, on return', async ({
  page,
}) => {
  const fixture = await setup(page);
  await page.route('**/api/tasks/1', async (r) => {
    if (r.request().method() !== 'PATCH') return r.fallback();
    const payload = r.request().postDataJSON();
    return r.fulfill({
      json: {
        data: {
          id: 1,
          title: payload.title,
          description: null,
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
          created_at: '2026-09-13T10:00:00Z',
          updated_at: '2026-09-13T10:00:00Z',
        },
      },
    });
  });
  await page.goto('/demo/list');
  await expect(row(page)).toContainText('Original task');

  await page.evaluate(async () => {
    const { updateTaskPartially } = await import('/src/actions/tmgr/tasks.ts');
    const { default: store } = await import('/src/store/index.js');
    const saved = await updateTaskPartially(1, { title: 'Edited task' });
    store.commit('updateSingleTask', saved);
  });
  await expect(row(page)).toContainText('Edited task');
  fixture.title = 'Edited task';
  await nav(page, 'Dashboard');
  await expect(kpi(page, 'completed')).toContainText('5');

  await watchFlashes(page);
  await nav(page, 'List');
  await expect(row(page)).toContainText('Edited task', SEEDED);
  await wait(DELAY);
  expect(await flashes(page)).toEqual([]);
});
