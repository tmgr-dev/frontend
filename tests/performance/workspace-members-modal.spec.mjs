import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const me = { id: 1, name: 'Test User', email: '1@example.test' };
const ann = { id: 2, name: 'Ann Lee', email: '2@example.test' };

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

const memberStats = (user) => ({
  ...user,
  has_avatar: false,
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
  member_since: '2026-01-01T10:00:00Z',
  assigned_count: 0,
  created_count: 0,
  commented_tasks_count: 0,
  active_days: 0,
  longest_streak: 0,
  team: { tracked_seconds: 0, done_count: 0, total_members: 2 },
  window: '7d',
});

const setup = async (page) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page);
  let members = [me, ann];
  const deletes = [];
  const state = { membersFail: false };
  await page.route('**/api/workspaces/1/members*', (route) =>
    state.membersFail
      ? route.fulfill({
          status: 500,
          json: { message: 'Members are unavailable' },
        })
      : route.fulfill({ json: { data: members } }),
  );
  await page.route('**/api/workspaces/1/members/*', (route) => {
    if (route.request().method() !== 'DELETE') return route.fallback();
    const id = Number(new URL(route.request().url()).pathname.split('/').pop());
    deletes.push(id);
    members = members.filter((m) => m.id !== id);
    return route.fulfill({ status: 204, body: '' });
  });
  await page.route('**/api/tasks/status/*', (route) =>
    route.fulfill({ json: { data: [task()] } }),
  );
  await page.route(
    (u) => u.pathname.startsWith('/api/') && u.pathname.includes('/dashboard'),
    (route) => {
      const path = new URL(route.request().url()).pathname;
      const stats = path.match(/dashboard\/members\/(\d+)$/);
      if (stats) {
        const user = [me, ann].find((m) => m.id === Number(stats[1]));
        return route.fulfill({ json: { data: memberStats(user) } });
      }
      if (/dashboard\/members\/\d+\/tasks$/.test(path))
        return route.fulfill({
          json: { data: [], total: 0, page: 1, per_page: 20 },
        });
      if (path.endsWith('activities'))
        return route.fulfill({
          json: {
            data: [],
            meta: { current_page: 1, per_page: 20, total: 0, last_page: 1 },
            links: {},
          },
        });
      return route.fulfill({ json: { data: {} } });
    },
  );
  return { deletes, state };
};

const openModal = async (page) => {
  await page.goto('/demo/board');
  await page
    .locator('button:has(span[title="Ann Lee"]):visible')
    .first()
    .click();
  const dialog = page.getByRole('dialog', { name: 'Workspace Members' });
  await expect(dialog).toBeVisible();
  await dialog.evaluate((el) =>
    Promise.all(el.getAnimations({ subtree: true }).map((a) => a.finished)),
  );
  return dialog;
};

const expectMemberPageInteractive = async (page, id) => {
  await expect(page).toHaveURL(new RegExp(`/demo/team/${id}$`));
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.locator('body')).not.toHaveCSS('pointer-events', 'none');
  const assigned = page.getByRole('tab', { name: /Assigned/ });
  await assigned.click();
  await expect(assigned).toHaveAttribute('aria-selected', 'true');
  await expect(page).toHaveURL(new RegExp(`/demo/team/${id}\\?tab=assigned$`));
};

test.describe('Workspace Members modal', () => {
  test.setTimeout(90_000);

  test('clicking another member opens their member page', async ({ page }) => {
    await setup(page);
    const dialog = await openModal(page);
    await dialog.getByRole('button', { name: 'Open Ann Lee' }).click();
    await expectMemberPageInteractive(page, 2);
  });

  test('clicking yourself (the owner) opens your member page', async ({
    page,
  }) => {
    await setup(page);
    const dialog = await openModal(page);
    const self = dialog.getByRole('button', { name: 'Open Test User' });
    await expect(self).toContainText('Owner');
    await self.click();
    await expectMemberPageInteractive(page, 1);
  });

  test('Enter on a focused member card navigates', async ({ page }) => {
    await setup(page);
    const dialog = await openModal(page);
    const card = dialog.getByRole('button', { name: 'Open Ann Lee' });
    await card.focus();
    await expect(card).toBeFocused();
    await page.keyboard.press('Enter');
    await expectMemberPageInteractive(page, 2);
  });

  test('clicking the row next to Remove opens the member page', async ({
    page,
  }) => {
    await setup(page);
    const dialog = await openModal(page);
    const row = dialog
      .getByRole('button', { name: 'Open Ann Lee' })
      .locator('xpath=..');
    const box = await row.boundingBox();
    const remove = await row
      .getByRole('button', { name: 'Remove' })
      .boundingBox();
    expect(box.x + box.width - 4).toBeGreaterThan(remove.x + remove.width);
    await row.click({ position: { x: box.width - 4, y: box.height / 2 } });
    await expect(page.getByRole('alertdialog')).toHaveCount(0);
    await expectMemberPageInteractive(page, 2);
  });

  test('Remove asks for confirmation and does not navigate', async ({
    page,
  }) => {
    const { deletes } = await setup(page);
    const dialog = await openModal(page);
    const before = page.url();
    const card = dialog.getByRole('button', { name: 'Open Ann Lee' });
    await expect(card).toBeVisible();
    await expect(card.getByRole('button')).toHaveCount(0);
    await expect(
      dialog.getByRole('button', { name: 'Open Test User' }),
    ).toBeVisible();
    await expect(dialog.getByRole('button', { name: 'Remove' })).toHaveCount(1);

    await dialog.getByRole('button', { name: 'Remove' }).click();
    const confirm = page.getByRole('alertdialog', { name: 'Remove Member' });
    await expect(confirm).toBeVisible();
    await expect(confirm).toContainText('Ann Lee');
    expect(page.url()).toBe(before);

    await confirm.getByRole('button', { name: 'Remove' }).click();
    await expect.poll(() => deletes).toEqual([2]);
    await expect(card).toHaveCount(0);
    await expect(dialog).toBeVisible();
    expect(page.url()).toBe(before);
  });

  test('a failed members reload shows the error and Retry, no cards', async ({
    page,
  }) => {
    const { state } = await setup(page);
    const dialog = await openModal(page);
    state.membersFail = true;
    await dialog.getByRole('button', { name: 'Remove' }).click();
    await page
      .getByRole('alertdialog', { name: 'Remove Member' })
      .getByRole('button', { name: 'Remove' })
      .click();

    await expect(dialog.getByText('Members are unavailable')).toBeVisible({
      timeout: 15_000,
    });
    const retry = dialog.getByRole('button', { name: 'Retry' });
    await expect(retry).toBeVisible();
    await expect(dialog.getByRole('button', { name: /^Open / })).toHaveCount(0);

    state.membersFail = false;
    await retry.click();
    await expect(
      dialog.getByRole('button', { name: 'Open Test User' }),
    ).toBeVisible();
    await expect(
      dialog.getByRole('button', { name: 'Open Ann Lee' }),
    ).toHaveCount(0);
  });
});
