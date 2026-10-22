import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const notification = (id, read = false) => ({
  id,
  user_id: 1,
  workspace_id: 1,
  activity_id: null,
  type: 'task_updated',
  title: `Notification ${id}`,
  message: `Message ${id}`,
  data: null,
  link: null,
  read_at: read ? '2026-10-01T11:00:00Z' : null,
  created_at: '2026-10-01T10:00:00Z',
  updated_at: '2026-10-01T10:00:00Z',
});

async function mockNotifications(page, { items, count, countStatus = 200 }) {
  const hits = { count: 0, readAll: 0, read: [], deleted: [] };
  await page.route('**/api/notifications**', (route) => {
    const req = route.request();
    const path = new URL(req.url()).pathname;
    if (path.endsWith('/unread-count')) {
      hits.count += 1;
      if (countStatus !== 200)
        return route.fulfill({ status: countStatus, json: { message: 'x' } });
      return route.fulfill({ json: { count } });
    }
    if (req.method() === 'POST' && path.endsWith('/read-all')) {
      hits.readAll += 1;
      return route.fulfill({ json: { count } });
    }
    if (req.method() === 'DELETE') {
      hits.deleted.push(Number(path.split('/').at(-1)));
      return route.fulfill({ status: 204 });
    }
    if (req.method() === 'POST' && path.endsWith('/read')) {
      hits.read.push(Number(path.split('/').at(-2)));
      return route.fulfill({ json: { notification: null } });
    }
    return route.fulfill({
      json: {
        data: items,
        current_page: 1,
        last_page: 1,
        per_page: 20,
        total: items.length,
      },
    });
  });
  return hits;
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await mockApp(page);
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
});

test('direct open shows Mark all as read; clicking it clears unread', async ({
  page,
}) => {
  const hits = await mockNotifications(page, {
    items: [notification(1), notification(2), notification(3, true)],
    count: 2,
  });
  await page.goto('/demo/notifications');
  const main = page.getByRole('main');
  await expect(main.locator('.notification-item')).toHaveCount(3);
  await expect(main.locator('.notification-item.unread')).toHaveCount(2);

  const markAll = main.getByRole('button', { name: 'Mark all as read' });
  await expect(markAll).toBeVisible();
  await markAll.click();

  await expect.poll(() => hits.readAll).toBe(1);
  await expect(markAll).toHaveCount(0);
  await expect(main.locator('.notification-item.unread')).toHaveCount(0);
});

test('bell and page share one unread-count request', async ({ page }) => {
  const hits = await mockNotifications(page, {
    items: [notification(1)],
    count: 1,
  });
  await page.goto('/demo/notifications');
  const bell = page.getByRole('button', { name: 'Notifications' }).first();
  await expect(bell).toContainText('1');
  await expect(
    page.getByRole('main').getByRole('button', { name: 'Mark all as read' }),
  ).toBeVisible();
  expect(hits.count).toBe(1);
});

test('bell badge follows delete, read and Mark all on the page (TM-712)', async ({
  page,
}) => {
  await mockNotifications(page, {
    items: [notification(1), notification(2), notification(3)],
    count: 3,
  });
  await page.goto('/demo/notifications');
  const bell = page.getByRole('button', { name: 'Notifications' }).first();
  const main = page.getByRole('main');
  await expect(bell).toContainText('3');
  await main
    .locator('.notification-item')
    .nth(0)
    .getByRole('button', { name: 'Delete notification' })
    .click();
  await expect(bell).toContainText('2');
  await main.locator('.notification-item').nth(0).click();
  await expect(bell).toContainText('1');
  await main.getByRole('button', { name: 'Mark all as read' }).click();
  await expect(bell.locator('span')).toHaveCount(0);
});

test('bell panel closes after View all navigates (TM-712)', async ({
  page,
}) => {
  await mockNotifications(page, { items: [notification(1)], count: 1 });
  await page.goto('/demo/board');
  await page.getByRole('button', { name: 'Notifications' }).first().click();
  const dropdown = page.locator('.notification-dropdown');
  await expect(dropdown).toBeVisible();
  await dropdown.getByRole('button', { name: 'View all' }).click();
  await expect(page).toHaveURL(/\/demo\/notifications$/);
  await expect(dropdown).toHaveCount(0);
});

test('all read: no Mark all as read button', async ({ page }) => {
  await mockNotifications(page, {
    items: [notification(1, true)],
    count: 0,
  });
  await page.goto('/demo/notifications');
  const main = page.getByRole('main');
  await expect(main.locator('.notification-item')).toHaveCount(1);
  await expect(
    main.getByRole('button', { name: 'Mark all as read' }),
  ).toHaveCount(0);
});

test('empty list shows the empty state and no button', async ({ page }) => {
  await mockNotifications(page, { items: [], count: 0 });
  await page.goto('/demo/notifications');
  const main = page.getByRole('main');
  await expect(
    main.getByRole('heading', { name: 'No notifications' }),
  ).toBeVisible();
  await expect(
    main.getByRole('button', { name: 'Mark all as read' }),
  ).toHaveCount(0);
});

test('unread-count failure still renders the list without the button', async ({
  page,
}) => {
  const hits = await mockNotifications(page, {
    items: [notification(1), notification(2)],
    count: 2,
    countStatus: 500,
  });
  const errors = [];
  page.on('pageerror', (err) => errors.push(err.message));
  const failed = page.waitForResponse((res) =>
    res.url().endsWith('/notifications/unread-count'),
  );
  await page.goto('/demo/notifications');
  expect((await failed).status()).toBe(500);
  await page.waitForLoadState('networkidle');
  // Page + bell; $axios does not retry 5xx (src/plugins/axios.ts).
  expect(hits.count).toBeLessThanOrEqual(2);
  const main = page.getByRole('main');
  await expect(main.locator('.notification-item')).toHaveCount(2);
  await expect(
    main.getByRole('button', { name: 'Mark all as read' }),
  ).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('dropdown is keyboard reachable: header, rows, delete, footer', async ({
  page,
}) => {
  const hits = await mockNotifications(page, {
    items: [notification(1), notification(2)],
    count: 2,
  });
  await page.goto('/demo/board');
  const bell = page.getByRole('button', { name: 'Notifications' }).first();
  await expect(bell).toContainText('2');
  await bell.focus();
  await page.keyboard.press('Enter');

  const dropdown = page.locator('.notification-dropdown');
  const rows = dropdown.locator('.notification-item');
  const deletes = dropdown.getByRole('button', { name: 'Delete notification' });
  const markAll = dropdown.getByRole('button', { name: 'Mark all as read' });
  await expect(rows).toHaveCount(2);
  await expect(
    page.getByRole('dialog', { name: 'Notifications' }),
  ).toBeVisible();
  await expect(dropdown).toBeFocused();
  await expect(markAll).not.toBeFocused();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Space');
  await page.keyboard.press('Escape');
  await expect(dropdown).toBeHidden();
  await expect(bell).toBeFocused();

  await page.keyboard.press('Enter');
  await expect(dropdown).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(markAll).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(rows.nth(0)).toBeFocused();
  await page.keyboard.press('Enter');
  await expect.poll(() => hits.read).toEqual([1]);
  await page.keyboard.press('Tab');
  await expect(deletes.nth(0)).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(rows.nth(1)).toBeFocused();
  await page.keyboard.press('Space');
  await expect.poll(() => hits.read).toEqual([1, 2]);
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');
  await expect(
    dropdown.getByRole('button', { name: 'View all' }),
  ).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/demo\/notifications$/);
  expect(hits.readAll).toBe(0);
});

test('page rows activate with Enter', async ({ page }) => {
  const hits = await mockNotifications(page, {
    items: [notification(1)],
    count: 1,
  });
  await page.goto('/demo/notifications');
  const row = page.getByRole('main').locator('.notification-item');
  await row.focus();
  await page.keyboard.press('Enter');
  await expect.poll(() => hits.read).toEqual([1]);
  await expect(row).not.toHaveClass(/\bunread\b/);
});

test('dropdown stays open after Mark all as read and delete', async ({
  page,
}) => {
  const hits = await mockNotifications(page, {
    items: [notification(1), notification(2)],
    count: 2,
  });
  await page.goto('/demo/board');
  const bell = page.getByRole('button', { name: 'Notifications' }).first();
  await expect(bell).toContainText('2');
  await bell.click();
  const dropdown = page.locator('.notification-dropdown');
  await dropdown.getByRole('button', { name: 'Mark all as read' }).click();
  await expect.poll(() => hits.readAll).toBe(1);
  await expect(dropdown).toBeVisible();
  await expect(dropdown.locator('.notification-item.unread')).toHaveCount(0);
  await dropdown
    .getByRole('button', { name: 'Delete notification' })
    .first()
    .click();
  await expect(dropdown).toBeVisible();
  await expect(dropdown.locator('.notification-item')).toHaveCount(1);
});

test('Enter on a focused delete button deletes without marking read', async ({
  page,
}) => {
  const hits = await mockNotifications(page, {
    items: [notification(1), notification(2)],
    count: 2,
  });
  await page.goto('/demo/notifications');
  const main = page.getByRole('main');
  await expect(main.locator('.notification-item')).toHaveCount(2);
  await main
    .getByRole('button', { name: 'Delete notification' })
    .first()
    .focus();
  await page.keyboard.press('Enter');
  await expect.poll(() => hits.deleted).toEqual([1]);
  await expect(main.locator('.notification-item')).toHaveCount(1);
  expect(hits.read).toEqual([]);
});
