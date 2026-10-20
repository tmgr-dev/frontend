import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const DAY = 86_400_000;
const todayUtc = () => {
  const now = new Date();
  return Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
};
const dayKey = (back) =>
  new Date(todayUtc() - back * DAY).toISOString().slice(0, 10);

// Heatmap: 2 actions a day this week, 1 a day before → 7d: 14 vs 7 (+100%), 30d: 37 vs 30 (+23%).
const heatmap = () => ({
  contributions: Array.from({ length: 365 }, (_, i) => {
    const back = 364 - i;
    const count = back < 7 ? 2 : back < 60 ? 1 : 0;
    return { date: dayKey(back), count, level: count };
  }),
  total_contributions: 67,
  streak: { current: 60, longest: 60 },
  year: new Date().getUTCFullYear(),
  weeks: 53,
});

const member = (id, name, activeDays, days) => ({
  id,
  name,
  email: `m${id}@example.test`,
  role: 'member',
  is_online: id === 1,
  last_activity_at: new Date().toISOString(),
  last_task: null,
  current_task: null,
  tracked_seconds: 1800,
  done_count: id,
  active_tasks: 1,
  completed_tasks: 1,
  comments_count: 0,
  activity_count: 1,
  daily_activity: Array.from({ length: days }, (_, i) => ({
    date: dayKey(days - 1 - i),
    count: days - 1 - i < activeDays ? 1 : 0,
  })),
  streak: 1,
});

const teamActivity = (window) => {
  const days = window === '30d' ? 30 : window === '7d' ? 7 : 1;
  const members = [
    member(1, 'Ann Lee', 3, days),
    member(2, 'Bob Ray', 0, days),
  ];
  return {
    members,
    total_members: 2,
    online_members: 1,
    active_today: 1,
    active_timers: 0,
    tracked_seconds: 3600,
    done_count: window === '30d' ? 12 : 5,
    window,
  };
};

// /time/daily: current period 30 min a day on Development, previous period 15 min a day.
const timeDaily = (from, to) => {
  const current = to === dayKey(0);
  const days = [];
  for (let t = Date.parse(from); t <= Date.parse(to); t += DAY) {
    const date = new Date(t).toISOString().slice(0, 10);
    const seconds = current ? 1800 : 900;
    days.push({
      date,
      total_s: seconds,
      items: [
        {
          task: { id: 1, key: 'DEV-1', title: 'Task', url: null },
          status: null,
          category: { id: 1, code: 'DEV', name: 'Development' },
          seconds,
          users: [{ id: 1, name: 'Ann Lee', seconds }],
        },
      ],
    });
  }
  return { from, to, tz: 'UTC', days };
};

const setup = async (page, { timeStatus = 200, quiet = false } = {}) => {
  const requests = { team: [], time: [] };
  const errors = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
  page.on('console', (m) => {
    if (m.type() === 'error' && !m.text().startsWith('Failed to load resource'))
      errors.push(m.text());
  });
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page, { taskCount: 1 });
  // Workspace modules come from /modules (feature-toggles is only the 404 fallback).
  await page.route('**/api/workspaces/1/modules', (r) =>
    r.fulfill({
      json: {
        data: {
          modules: ['board', 'dashboard', 'exports', 'task.countdown'].map(
            (key) => ({ key, enabled: true }),
          ),
        },
      },
    }),
  );
  await page.route(
    (u) => u.pathname.startsWith('/api/') && u.pathname.includes('/dashboard/'),
    (r) => {
      const url = new URL(r.request().url());
      const p = url.pathname;
      if (p.endsWith('heatmap'))
        return r.fulfill({
          json: {
            data: quiet ? { ...heatmap(), contributions: [] } : heatmap(),
          },
        });
      if (p.endsWith('team-activity')) {
        const window = url.searchParams.get('window');
        requests.team.push(window);
        const data = teamActivity(window);
        if (quiet)
          data.members = data.members.map((m) => ({
            ...m,
            daily_activity: [],
            done_count: 0,
          }));
        if (quiet) data.done_count = 0;
        return r.fulfill({ json: { data } });
      }
      if (p.endsWith('activities'))
        return r.fulfill({
          json: {
            data: [],
            meta: { current_page: 1, per_page: 20, total: 0, last_page: 1 },
            links: {},
          },
        });
      return r.fulfill({ json: { data: {} } });
    },
  );
  await page.route('**/api/workspaces/1/time/daily**', (r) => {
    const url = new URL(r.request().url());
    const from = url.searchParams.get('from');
    const to = url.searchParams.get('to');
    requests.time.push({ from, to, tz: url.searchParams.get('tz') });
    if (timeStatus === 403)
      return r.fulfill({
        status: 403,
        json: {
          error: 'feature_disabled',
          feature: 'exports',
          message: 'Module `exports` is off',
        },
      });
    const data = timeDaily(from, to);
    if (quiet) data.days = [];
    return r.fulfill({ json: { data } });
  });
  return { requests, errors };
};

const kpi = (page, key) => page.locator(`[data-kpi="${key}"]`);

test('KPI row, chart and categories follow the period selector', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { requests, errors } = await setup(page);
  await page.goto('/demo/dashboard');

  await expect(kpi(page, 'activity')).toContainText('14');
  await expect(kpi(page, 'activity').locator('[data-kpi-delta]')).toContainText(
    '+100%',
  );
  await expect(kpi(page, 'activity').locator('[data-kpi-delta]')).toContainText(
    'vs previous 7 days',
  );
  await expect(kpi(page, 'members')).toContainText('1');
  await expect(kpi(page, 'members').locator('[data-kpi-delta]')).toHaveCount(0);
  await expect(kpi(page, 'tracked')).toContainText('3h 30m');
  await expect(kpi(page, 'tracked').locator('[data-kpi-delta]')).toContainText(
    '+100%',
  );
  await expect(kpi(page, 'completed')).toContainText('5');
  await expect(kpi(page, 'completed').locator('[data-sparkline]')).toHaveCount(
    0,
  );
  await expect(page.locator('[data-kpi]')).toHaveCount(4);

  const chart = page.locator('[data-activity-chart]');
  await expect(chart.locator('[role="img"]')).toHaveAttribute(
    'aria-label',
    /Activity/,
  );
  await expect(chart.locator('table tbody tr')).toHaveCount(7);
  await expect(page.locator('[data-top-categories]')).toContainText(
    'Development',
  );
  await expect(page.locator('[data-top-categories]')).toContainText('3h 30m');

  expect(requests.team).toEqual(['7d']);
  expect(requests.time).toEqual(
    expect.arrayContaining([
      { from: dayKey(6), to: dayKey(0), tz: 'UTC' },
      { from: dayKey(13), to: dayKey(7), tz: 'UTC' },
    ]),
  );

  await page.getByRole('button', { name: 'Last 30 days' }).click();
  await expect(kpi(page, 'activity')).toContainText('37');
  await expect(kpi(page, 'activity').locator('[data-kpi-delta]')).toContainText(
    '+23%',
  );
  await expect(kpi(page, 'completed')).toContainText('12');
  await expect(kpi(page, 'tracked')).toContainText('15h 00m');
  await expect(chart.locator('table tbody tr')).toHaveCount(30);
  expect(requests.team).toContain('30d');
  expect(requests.time).toEqual(
    expect.arrayContaining([
      { from: dayKey(29), to: dayKey(0), tz: 'UTC' },
      { from: dayKey(59), to: dayKey(30), tz: 'UTC' },
    ]),
  );
  expect(errors).toEqual([]);
});

test('time/daily 403 leaves tracked hours without sparkline and hides categories', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { errors } = await setup(page, { timeStatus: 403 });
  await page.goto('/demo/dashboard');
  await expect(kpi(page, 'activity')).toContainText('14');
  await expect(kpi(page, 'tracked')).toContainText('1h 00m');
  await expect(kpi(page, 'tracked').locator('[data-sparkline]')).toHaveCount(0);
  await expect(kpi(page, 'tracked').locator('[data-kpi-delta]')).toHaveCount(0);
  await expect(page.locator('[data-top-categories]')).toHaveCount(0);
  await expect(page.getByRole('alert')).toHaveCount(0);
  const overflow = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  );
  // scrollbar-gutter: stable on <html> makes an unscrolled page read -7.
  expect(overflow).toBeLessThanOrEqual(0);
  expect(errors).toEqual([]);
});

test('an empty workspace shows empty states, not broken charts', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { errors } = await setup(page, { quiet: true });
  await page.goto('/demo/dashboard');
  await expect(kpi(page, 'activity')).toContainText('0');
  await expect(kpi(page, 'activity').locator('[data-kpi-delta]')).toHaveCount(
    0,
  );
  await expect(page.locator('[data-chart-empty]')).toBeVisible();
  await expect(page.locator('[data-categories-empty]')).toBeVisible();
  expect(errors).toEqual([]);
});

// D-009: the widget's own window (Today / 30d) must not blank the page-period KPIs and chart.
for (const tab of ['Today', '30d'])
  test(`widget window ${tab} keeps the 7-day KPIs and chart`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const { requests, errors } = await setup(page);
    await page.goto('/demo/dashboard');
    await expect(kpi(page, 'members')).toContainText('of 2 members');
    await expect(kpi(page, 'completed')).toContainText('5');

    await page.getByRole('tab', { name: tab }).click();
    await expect.poll(() => requests.team.length).toBe(2);
    await expect(page.locator('.team-activity-widget')).toContainText(
      tab === 'Today' ? 'activity today' : 'last 30 days',
    );

    await expect(kpi(page, 'members')).toContainText('of 2 members', {
      timeout: 5000,
    });
    await expect(kpi(page, 'completed')).toContainText('5');
    await expect(
      page.locator('[data-activity-chart] [role="img"]'),
    ).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'Last 7 days' }),
    ).toHaveAttribute('aria-pressed', 'true');
    expect(errors).toEqual([]);
  });

test('member names stay readable in the members table at 1280', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await setup(page);
  await page.goto('/demo/dashboard');
  const name = page.locator('.team-activity-widget h4', { hasText: 'Ann Lee' });
  await expect(name).toBeVisible();
  const { clientWidth, scrollWidth } = await name.evaluate((el) => ({
    clientWidth: el.clientWidth,
    scrollWidth: el.scrollWidth,
  }));
  expect(clientWidth).toBeGreaterThanOrEqual(scrollWidth);
});

test('chart tracked total matches the KPI (rounded after summing)', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await setup(page);
  await page.route('**/api/workspaces/1/time/daily**', (r) => {
    const url = new URL(r.request().url());
    const data = timeDaily(
      url.searchParams.get('from'),
      url.searchParams.get('to'),
    );
    for (const d of data.days) {
      d.total_s = 2700;
      d.items[0].seconds = 2700;
    }
    return r.fulfill({ json: { data } });
  });
  await page.goto('/demo/dashboard');
  await expect(kpi(page, 'tracked')).toContainText('5h 15m');
  await expect(
    page.locator('[data-activity-chart] [role="img"]'),
  ).toHaveAttribute('aria-label', /Tracked hours: total 5\.3 h/);
});

test('a slow 30-day /time/daily keeps the tracked series loading, not stale', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await setup(page);
  let release;
  const gate = new Promise((resolve) => (release = resolve));
  await page.route('**/api/workspaces/1/time/daily**', async (r) => {
    const url = new URL(r.request().url());
    const from = url.searchParams.get('from');
    if (from === dayKey(29)) await gate;
    return r.fulfill({
      json: { data: timeDaily(from, url.searchParams.get('to')) },
    });
  });
  await page.goto('/demo/dashboard');
  await expect(kpi(page, 'tracked')).toContainText('3h 30m');
  await page.getByRole('button', { name: 'Last 30 days' }).click();
  await expect(kpi(page, 'activity')).toContainText('37');
  await expect(kpi(page, 'tracked')).not.toContainText('3h 30m');
  await expect(page.locator('[data-activity-chart] table')).toHaveCount(0);
  release();
  await expect(kpi(page, 'tracked')).toContainText('15h 00m');
  await expect(
    page.locator('[data-activity-chart] table tbody tr'),
  ).toHaveCount(30);
});

test('no /time/daily requests while the dashboard module is off', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { requests } = await setup(page);
  await page.route('**/api/workspaces/1/modules', (r) =>
    r.fulfill({
      json: {
        data: {
          modules: [
            { key: 'board', enabled: true },
            { key: 'dashboard', enabled: false },
            { key: 'exports', enabled: true },
            { key: 'task.countdown', enabled: true },
          ],
        },
      },
    }),
  );
  await page.goto('/demo/dashboard');
  await expect(
    page.getByRole('heading', { name: 'Dashboard', level: 2 }),
  ).toBeVisible();
  await page.waitForTimeout(1500);
  await expect(page.locator('[data-kpi]')).toHaveCount(0);
  expect(requests.time).toEqual([]);
});
