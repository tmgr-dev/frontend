import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const me = { id: 1, name: 'Test User', has_avatar: false };
const ann = { id: 2, name: 'Ann Lee', has_avatar: false };

const task = () => ({
  id: 1,
  title: 'Original task',
  description: null,
  description_json: null,
  status: 'created',
  status_id: 1,
  workspace_id: 1,
  user_id: 1,
  user: me,
  assignees: [me, ann],
  persona_assignees: [],
  settings: [],
  checkpoints: [],
  common_time: 0,
  start_time: 0,
  approximately_time: 0,
  project_category_id: null,
  created_at: '2026-09-13T10:00:00Z',
  updated_at: '2026-09-13T10:00:00Z',
});

const teamMember = (user) => ({
  ...user,
  email: `${user.id}@example.test`,
  role: 'member',
  is_online: true,
  last_activity_at: null,
  last_task: null,
  current_task: null,
  tracked_seconds: 0,
  done_count: 0,
  active_tasks: 0,
  completed_tasks: 0,
  comments_count: 0,
  activity_count: 0,
  daily_activity: [],
  streak: 0,
});

const toggle = (key) => ({
  key,
  name: key,
  group: 'pages',
  type: 'boolean',
  enabled: true,
});

const withMembers = async (page) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page);
  await page.route('**/api/**/feature-toggles', (route) =>
    route.fulfill({
      json: {
        data: route.request().url().includes('/user/')
          ? {
              default_landing_page: {
                key: 'default_landing_page',
                name: 'Default Landing Page',
                group: 'general',
                type: 'select',
                options: ['list', 'board', 'dashboard', 'daily_routines'],
                value: 'list',
              },
              'board.user_filter': toggle('board.user_filter'),
            }
          : {
              board: toggle('board'),
              dashboard: toggle('dashboard'),
              'task.assignees': toggle('task.assignees'),
            },
      },
    }),
  );
  await page.route('**/api/workspaces/1/members*', (route) =>
    route.fulfill({ json: { data: [me, ann] } }),
  );
  await page.route('**/api/tasks/1', (route) =>
    route.fulfill({ json: { data: task() } }),
  );
  await page.route('**/api/tasks/current*', (route) =>
    route.fulfill({
      json: {
        data: [task()],
        meta: {
          current_page: 1,
          per_page: 10,
          total: 1,
          last_page: 1,
          from: 1,
          to: 1,
        },
      },
    }),
  );
  await page.route('**/api/tasks/status/*', (route) =>
    route.fulfill({ json: { data: [task()] } }),
  );
  await page.route('**/api/tasks/1/comments/', (route) =>
    route.fulfill({
      json: {
        data: [
          {
            id: 7,
            message: 'Looks good to me',
            user: ann,
            created_at: '2026-09-13T10:00:00Z',
            updated_at: '2026-09-13T10:00:00Z',
            reactions: [],
          },
        ],
      },
    }),
  );
  await page.route(
    (u) => u.pathname.startsWith('/api/') && u.pathname.includes('/dashboard'),
    (route) => {
      const path = new URL(route.request().url()).pathname;
      if (path.endsWith('team-activity'))
        return route.fulfill({
          json: {
            data: {
              members: [teamMember(me), teamMember(ann)],
              total_members: 2,
              online_members: 2,
              active_today: 2,
              active_timers: 0,
              tracked_seconds: 0,
              done_count: 0,
              window: '7d',
            },
          },
        });
      if (path.endsWith('activities'))
        return route.fulfill({
          json: {
            data: [],
            meta: { current_page: 1, per_page: 20, total: 0, last_page: 1 },
            links: {},
          },
        });
      if (path.endsWith('recent-tasks'))
        return route.fulfill({ json: { data: [] } });
      if (path.endsWith('heatmap'))
        return route.fulfill({
          json: {
            data: {
              contributions: [],
              total_contributions: 0,
              streak: { current: 0, longest: 0 },
              year: 2026,
              weeks: 53,
            },
          },
        });
      return route.fulfill({ json: { data: {} } });
    },
  );
};

const avatarOf = (scope, name) =>
  scope.locator(`span.rounded-full[title="${name}"]`);

test('a human comment author avatar opens the member page', async ({
  page,
}) => {
  await withMembers(page);
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const comment = page.locator('[data-comment-id="7"]');
  await expect(comment).toBeVisible();

  const link = comment.getByRole('link', { name: 'Open Ann Lee' });
  await expect(link).toBeVisible();
  await expect(avatarOf(link, 'Ann Lee')).toHaveCount(1);
  await link.click();
  await expect(page).toHaveURL(/\/demo\/team\/2$/);
});

test('a dashboard team member avatar opens the member page', async ({
  page,
}) => {
  await withMembers(page);
  await page.goto('/demo/dashboard');
  const member = page
    .locator('[data-tour="dashboard.member"]')
    .filter({ hasText: 'Ann Lee' });
  const link = member.getByRole('link', { name: 'Open Ann Lee' });
  await expect(link).toBeVisible();
  await expect(avatarOf(link, 'Ann Lee')).toHaveCount(1);
  await link.click();
  await expect(page).toHaveURL(/\/demo\/team\/2$/);
});
