import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const LONG = 'Extraordinarilylongtitlewithoutanyspaces'.repeat(6);

const heatmap = () => {
  const contributions = [];
  const start = new Date();
  start.setDate(start.getDate() - 364);
  for (let i = 0; i < 365; i++) {
    const d = new Date(start);
    d.setDate(start.getDate() + i);
    contributions.push({
      date: d.toISOString().slice(0, 10),
      count: i % 5,
      level: i % 5,
    });
  }
  return {
    contributions,
    total_contributions: 700,
    streak: { current: 3, longest: 9 },
    year: new Date().getFullYear(),
    weeks: 53,
  };
};

const member = (id) => ({
  id,
  name: `${LONG} ${id}`,
  email: `m${id}@example.test`,
  role: 'member',
  is_online: true,
  last_activity_at: null,
  last_task: { id, title: LONG },
  current_task: { id, title: LONG, timer_running: true },
  tracked_seconds: 100,
  done_count: 1,
  active_tasks: 2,
  completed_tasks: 1,
  comments_count: 1,
  activity_count: 1,
  daily_activity: [],
  streak: 1,
});

const activity = (id) => ({
  id,
  type: 'task_created',
  title: LONG,
  subject_name: LONG,
  description: LONG,
  user: { id: 1, name: LONG },
  workspace_id: 1,
  metadata: {},
  created_at: '2026-10-01T10:00:00Z',
  timestamp_human: '1 day ago',
});

const mockDashboard = async (page) => {
  await page.route(
    (u) =>
      u.pathname.startsWith('/api/') &&
      u.pathname.endsWith('feature-toggles') &&
      !u.pathname.includes('/user'),
    (r) =>
      r.fulfill({
        json: { data: { board: { enabled: true }, dashboard: { enabled: true } } },
      }),
  );
  await page.route(
    (u) => u.pathname.startsWith('/api/') && u.pathname.includes('/dashboard/'),
    (r) => {
      const p = new URL(r.request().url()).pathname;
      if (p.endsWith('activities'))
        return r.fulfill({
          json: {
            data: Array.from({ length: 20 }, (_, i) => activity(i + 1)),
            meta: { current_page: 1, per_page: 20, total: 20, last_page: 1 },
            links: {},
          },
        });
      if (p.endsWith('heatmap')) return r.fulfill({ json: { data: heatmap() } });
      if (p.endsWith('team-activity'))
        return r.fulfill({
          json: {
            data: {
              members: Array.from({ length: 5 }, (_, i) => member(i + 1)),
              total_members: 5,
              online_members: 5,
              active_today: 5,
              active_timers: 5,
              tracked_seconds: 1,
              done_count: 1,
              window: 'today',
            },
          },
        });
      if (p.endsWith('recent-tasks'))
        return r.fulfill({
          json: {
            data: Array.from({ length: 5 }, (_, i) => ({
              id: i + 1,
              title: LONG,
              user: { id: 1, name: LONG },
              updated_at: '2026-10-01T10:00:00Z',
              updated_at_human: '1 day ago',
              timer_running: false,
              is_overdue: false,
            })),
          },
        });
      if (p.endsWith('statistics'))
        return r.fulfill({
          json: {
            data: {
              total_tasks: 99999,
              active_tasks: 5,
              completed_today: 1,
              completed_week: 1,
              completed_month: 1,
              time_today: 1,
              time_week: 1,
              time_month: 1,
              time_total: 1,
              team_members: 5,
              overdue_tasks: 1,
              daily_routine_completion_rate: 10,
            },
          },
        });
      return r.fulfill({ json: { data: {} } });
    },
  );
};

for (const width of [1280, 1440, 1920]) {
  for (const sidebar of ['open', 'closed']) {
    test(`dashboard with long unbroken titles fits the viewport @${width} sidebar ${sidebar}`, async ({
      page,
    }) => {
      await page.setViewportSize({ width, height: 900 });
      await mockApp(page, { taskCount: 3 });
      await mockDashboard(page);
      await page.goto('/demo/dashboard');
      await page.locator('[data-page-header] h1').first().waitFor();
      await page.getByText('by Extraordinarily').first().waitFor();
      if (sidebar === 'closed') {
        await page.keyboard.press('Control+b');
        await page.waitForTimeout(500);
      }
      const m = await page.evaluate(() => {
        const de = document.documentElement;
        const main = document.querySelector('main');
        return {
          page: de.scrollWidth - de.clientWidth,
          main: main.scrollWidth - main.clientWidth,
        };
      });
      expect(m).toEqual({ page: 0, main: 0 });
    });
  }
}
