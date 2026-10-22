import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

// TM-241: "All time" window on the dashboard widget and the member page, against a mocked API.
const SHOTS = process.env.TM241_SHOTS_DIR;
const DAY = 86_400_000;
const todayUtc = () => {
  const now = new Date();
  return Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
};
const dayKey = (back) =>
  new Date(todayUtc() - back * DAY).toISOString().slice(0, 10);

const daily = (n) =>
  Array.from({ length: n }, (_, i) => ({
    date: dayKey(n - 1 - i),
    count: i % 3 === 0 ? 2 : 0,
  }));

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

const widgetMember = (id, name, days) => ({
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
  daily_activity: daily(days),
  streak: 1,
});

const team = (window, { empty = false } = {}) => ({
  members: empty
    ? []
    : [
        widgetMember(1, 'Ann Lee', window === 'all' ? 30 : 7),
        widgetMember(2, 'Bob Ray', window === 'all' ? 30 : 7),
      ],
  total_members: empty ? 0 : 2,
  online_members: empty ? 0 : 1,
  active_today: empty ? 0 : 1,
  active_timers: 0,
  tracked_seconds: window === 'all' ? 360000 : 3600,
  done_count: window === 'all' ? 40 : 5,
  window,
});

const memberStats = (window) => ({
  ...widgetMember(2, 'Bob Ray', 30),
  has_avatar: false,
  tracked_seconds: window === 'all' ? 360000 : 1800,
  done_count: window === 'all' ? 77 : 3,
  streak: 4,
  longest_streak: 9,
  active_days: 11,
  assigned_count: 2,
  created_count: 1,
  commented_tasks_count: 0,
  member_since: '2025-01-01T00:00:00Z',
  team: { tracked_seconds: 720000, done_count: 200, total_members: 2 },
  window,
});

const setup = async (
  page,
  { rejectAll = false, empty = false, scheme = null } = {},
) => {
  const req = { team: [], stats: [], tasks: [] };
  const errors = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
  page.on('console', (m) => {
    if (m.type() === 'error' && !m.text().startsWith('Failed to load resource'))
      errors.push(m.text());
  });
  await page.addInitScript((dark) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    if (dark) localStorage.setItem('colorScheme', 'dark');
  }, scheme === 'dark');
  if (scheme) await page.emulateMedia({ colorScheme: scheme });
  await mockApp(page, { taskCount: 1 });
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
      const window = url.searchParams.get('window');
      const reject = () =>
        r.fulfill({
          status: 400,
          json: {
            message: "Invalid window 'all', expected one of: today, 7d, 30d",
          },
        });
      if (p.endsWith('team-activity')) {
        req.team.push(window);
        if (rejectAll && window === 'all') return reject();
        return r.fulfill({ json: { data: team(window, { empty }) } });
      }
      if (/\/members\/\d+$/.test(p)) {
        req.stats.push(window);
        if (rejectAll && window === 'all') return reject();
        return r.fulfill({
          json: { data: memberStats((window ?? '7d').toLowerCase()) },
        });
      }
      if (/\/members\/\d+\/tasks$/.test(p)) {
        req.tasks.push(window);
        if (rejectAll && window === 'all') return reject();
        return r.fulfill({
          json: {
            data: {
              data: [],
              total: 0,
              page: 1,
              per_page: 20,
              tab: url.searchParams.get('tab'),
              window,
            },
          },
        });
      }
      if (p.endsWith('heatmap'))
        return r.fulfill({ json: { data: heatmap() } });
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
  await page.route('**/api/workspaces/1/time/daily**', (r) =>
    r.fulfill({
      json: { data: { from: '', to: '', tz: 'UTC', days: [] } },
    }),
  );
  return { req, errors };
};

const shot = async (page, name) => {
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/${name}.png` });
};

const widget = (page) => page.locator('.team-activity-widget');

test('widget: 4 tabs, All time requests window=all with all-time labels', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { req, errors } = await setup(page);
  await page.goto('/demo/dashboard');
  await expect(widget(page)).toContainText('Ann Lee');
  await expect(widget(page).getByRole('tab')).toHaveText([
    'Today',
    '7d',
    '30d',
    'All time',
  ]);
  const expected = {
    Today: ['Members’ activity today', 'Today'],
    '7d': ['Members’ activity in the last 7 days', '7d'],
    '30d': ['Members’ activity in the last 30 days', '30d'],
    'All time': ['Members’ activity, all time', 'All time'],
  };
  let n = 1; // initial 7d load
  for (const [tab, [subtitle, label]] of Object.entries(expected)) {
    await page.getByRole('tab', { name: tab, exact: true }).click();
    n += 1;
    await expect.poll(() => req.team.length).toBe(n);
    await expect(
      widget(page).getByRole('tab', { name: tab, exact: true }),
    ).toHaveAttribute('aria-selected', 'true');
    await expect(
      widget(page).getByText(subtitle, { exact: true }),
    ).toBeVisible();
    await expect(widget(page)).toContainText(`Tracked · ${label}`);
    await expect(widget(page)).toContainText(`Done · ${label}`);
    if (tab === 'All time') await shot(page, 'widget-all-1440-light');
  }
  expect(req.team).toEqual(['7d', 'today', '7d', '30d', 'all']);
  expect(errors).toEqual([]);
});

test('widget: window=all 400 falls back to 30d, no error, no toast', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { req, errors } = await setup(page, { rejectAll: true });
  await page.goto('/demo/dashboard');
  await expect(widget(page)).toContainText('Ann Lee');
  await page.getByRole('tab', { name: 'All time' }).click();
  await expect.poll(() => req.team).toEqual(['7d', 'all', '30d']);
  await expect(widget(page).getByRole('tab', { name: '30d' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(widget(page)).toContainText('Ann Lee');
  await expect(widget(page)).toContainText('Tracked · 30d');
  await expect(
    page.getByRole('region', { name: /Notifications/ }).getByRole('listitem'),
  ).toHaveCount(0);
  await expect(page.getByText(/invalid window/i)).toHaveCount(0);
  await shot(page, 'widget-fallback-1440-light');
  expect(errors).toEqual([]);
});

test('widget: zero members under All time shows the empty state', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { req, errors } = await setup(page, { empty: true });
  await page.goto('/demo/dashboard');
  await page.getByRole('tab', { name: 'All time' }).click();
  await expect.poll(() => req.team).toEqual(['7d', 'all']);
  await expect(widget(page)).toContainText('No team members');
  await expect(widget(page)).toContainText('Members’ activity, all time');
  await shot(page, 'widget-empty-all-1440-light');
  expect(errors).toEqual([]);
});

for (const [scheme, width, height] of [
  ['dark', 1440, 900],
  ['light', 390, 844],
  ['dark', 390, 844],
]) {
  test(`widget: All time renders at ${width} ${scheme}`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    const { req, errors } = await setup(page, { scheme });
    await page.goto('/demo/dashboard');
    await expect(widget(page)).toContainText('Ann Lee');
    await page.getByRole('tab', { name: 'All time' }).click();
    await expect.poll(() => req.team).toEqual(['7d', 'all']);
    await expect(widget(page)).toContainText('Members’ activity, all time');
    await expect(widget(page)).toContainText('Tracked · All time');
    await expect(page.locator('html')).toHaveClass(
      scheme === 'dark' ? /dark/ : /^((?!dark).)*$/,
    );
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);
    const tabs = await widget(page).getByRole('tablist').boundingBox();
    expect(tabs.x + tabs.width).toBeLessThanOrEqual(width);
    await widget(page).scrollIntoViewIfNeeded();
    await shot(page, `widget-all-${width}-${scheme}`);
    expect(errors).toEqual([]);
  });
}

const hint = (page, label) =>
  page.locator('dl > div', { has: page.locator('dt', { hasText: label }) });

for (const raw of ['all', 'ALL'])
  test(`member page: ?window=${raw} loads All time, stats+tasks send window=all`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const { req, errors } = await setup(page);
    await page.goto(`/demo/team/2?window=${raw}`);
    await expect(
      page.getByRole('heading', { name: 'Bob Ray' }).first(),
    ).toBeVisible();
    const group = page.getByRole('group', { name: 'Time window' });
    await expect(group.getByRole('button')).toHaveText([
      'Today',
      '7 days',
      '30 days',
      'All time',
    ]);
    await expect(group.getByRole('button', { name: 'All time' })).toHaveClass(
      /bg-brand/,
    );
    await expect(hint(page, 'Streak')).toContainText(
      'longest 9d · last 30 days',
    );
    await expect(hint(page, 'Activity')).toContainText(
      '11 active days · last 30 days',
    );
    await expect(page.getByText('Last 30 days', { exact: true })).toBeVisible();
    expect(req.stats).toEqual(['all']);
    expect(req.tasks).toEqual(['all']);
    await shot(page, `member-all-${raw}-1440-light`);
    expect(errors).toEqual([]);
  });

test('member page: qualifier only under All time; click switches window', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { req, errors } = await setup(page);
  await page.goto('/demo/team/2');
  await expect(
    page.getByRole('heading', { name: 'Bob Ray' }).first(),
  ).toBeVisible();
  await expect(hint(page, 'Streak')).toContainText('longest 9d');
  await expect(hint(page, 'Streak')).not.toContainText('last 30 days');
  await expect(hint(page, 'Activity')).not.toContainText('last 30 days');
  await page.getByRole('button', { name: 'All time', exact: true }).click();
  await expect(page).toHaveURL(/window=all/);
  await expect(hint(page, 'Streak')).toContainText('· last 30 days');
  await page.getByRole('button', { name: '30 days', exact: true }).click();
  await expect(hint(page, 'Streak')).not.toContainText('last 30 days');
  expect(req.stats).toEqual(['7d', 'all', '30d']);
  expect(req.tasks).toEqual(['7d', 'all', '30d']);
  expect(errors).toEqual([]);
});

for (const [scheme, width, height] of [
  ['dark', 1440, 900],
  ['light', 390, 844],
  ['dark', 390, 844],
])
  test(`member page: All time renders at ${width} ${scheme}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height });
    const { errors } = await setup(page, { scheme });
    await page.goto('/demo/team/2?window=all');
    await expect(hint(page, 'Streak')).toContainText('· last 30 days');
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);
    await shot(page, `member-all-${width}-${scheme}`);
    expect(errors).toEqual([]);
  });

test('member page: 400 for window=all switches URL to window=30d', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { req, errors } = await setup(page, { rejectAll: true });
  await page.goto('/demo/team/2?window=all');
  await expect(page).toHaveURL(/window=30d/);
  await expect(
    page.getByRole('heading', { name: 'Bob Ray' }).first(),
  ).toBeVisible();
  await expect(
    page.getByRole('group', { name: 'Time window' }).getByRole('button', {
      name: '30 days',
    }),
  ).toHaveClass(/bg-brand/);
  await expect(hint(page, 'Streak')).not.toContainText('last 30 days');
  expect(req.stats.at(-1)).toBe('30d');
  expect(req.tasks.at(-1)).toBe('30d');
  await expect(page.getByText(/invalid window/i)).toHaveCount(0);
  await shot(page, 'member-fallback-1440-light');
  expect(errors).toEqual([]);
});
