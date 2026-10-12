import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const today = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
};

const stretch = () => ({
  task_id: 31,
  title: 'Stretch',
  description: null,
  routine_category: { id: 'health', name: 'Health', color: '#22c55e' },
  date: today(),
  time: '08:15',
  duration_min: 20,
  reminder_min: 15,
  completed: false,
  status: 'pending',
  instance_id: null,
  virtual: true,
  frequency: 'DAILY',
  scheduled_for: null,
});

const mockRoutines = async (
  page,
  { convertStatus = 200, quickStatus = 200, putStatus = 200, entries = () => [stretch()] } = {},
) => {
  const writes = [];
  // routines.first auto-offers 600ms after landing on /routines and covers the page.
  await page.addInitScript(() => sessionStorage.setItem('tmgr:tours:auto', 'true'));
  await page.route('**/api/daily-routines/**', (route) => {
    const request = route.request();
    const method = request.method();
    const path = new URL(request.url()).pathname.replace(/^.*\/api\/daily-routines\//, '');
    if (method !== 'GET') writes.push({ method, path, body: request.postDataJSON() });
    if (method === 'POST' && path === 'tasks/quick')
      return quickStatus === 200
        ? route.fulfill({ json: { data: { id: 40, title: 'Stretch (copy)' } } })
        : route.fulfill({ status: quickStatus, json: { message: 'Quota reached' } });
    if (method === 'PUT' && /^tasks\/\d+$/.test(path) && putStatus !== 200)
      return route.fulfill({ status: putStatus, json: { message: 'Save failed' } });
    if (method === 'PUT' && /^tasks\/\d+$/.test(path))
      return route.fulfill({ json: { data: { id: Number(path.split('/')[1]) } } });
    if (method === 'POST' && /^tasks\/\d+\/convert$/.test(path))
      return convertStatus === 200
        ? route.fulfill({ json: { data: { id: 77, title: 'Stretch' } } })
        : route.fulfill({ status: convertStatus, json: { message: 'Conversion exploded' } });
    if (path.startsWith('expand/stats')) return route.fulfill({ json: { data: {} } });
    if (path.startsWith('expand')) {
      const params = new URL(request.url()).searchParams;
      return route.fulfill({ json: { data: entries(params.get('from'), params.get('to')) } });
    }
    if (/^tasks\/\d+$/.test(path))
      return route.fulfill({
        json: {
          data: {
            id: 31,
            title: 'Stretch',
            description: 'Neck too',
            routine_category: 'health',
            approximately_time: 20,
            scheduled_date: null,
            scheduled_time: null,
            recurring: false,
          },
        },
      });
    return route.fulfill({ json: { data: [] } });
  });
  return writes;
};

const openMenu = async (page) => {
  await page.goto('/routines');
  await page.getByText('Stretch', { exact: true }).first().click({ button: 'right' });
};

test('duplicate creates a copy with the routine fields and " (copy)" title', async ({ page }) => {
  await mockApp(page);
  const writes = await mockRoutines(page);
  await openMenu(page);
  await page.getByRole('button', { name: 'Duplicate' }).click();

  await expect.poll(() => writes.filter((w) => w.method === 'PUT').length).toBe(1);
  const quick = writes.find((w) => w.path === 'tasks/quick');
  expect(quick.body).toMatchObject({ title: 'Stretch (copy)', time: '08:15', category: 'health' });
  const put = writes.find((w) => w.method === 'PUT');
  expect(put.path).toBe('tasks/40');
  expect(put.body).toMatchObject({
    title: 'Stretch (copy)',
    description: 'Neck too',
    routine_category: 'health',
    is_recurring: true,
    approximately_time: 20,
    recurrence: {
      frequency: 'DAILY',
      reminder_min: 15,
      duration_min: 20,
      time: { hours: 8, minutes: 15 },
    },
  });
});

test('a failed duplicate shows a toast', async ({ page }) => {
  await mockApp(page);
  const writes = await mockRoutines(page, { quickStatus: 422 });
  await openMenu(page);
  await page.getByRole('button', { name: 'Duplicate' }).click();
  await expect(page.getByText('Could not duplicate the routine', { exact: true })).toBeVisible();
  await expect(page.getByText('Quota reached', { exact: true })).toBeVisible();
  expect(writes.filter((w) => w.method === 'PUT')).toEqual([]);
});

test('reopening Edit shows the saved reminder and repeat of a routine', async ({ page }) => {
  await mockApp(page);
  const writes = await mockRoutines(page);
  await page.goto('/routines');
  await page.getByText('Stretch', { exact: true }).first().click();
  const reminder = page.locator('[data-tour="routine.reminder"]');
  await expect(reminder.getByRole('button', { name: '15m' })).toHaveClass(/border-brand/);
  await expect(page.getByRole('button', { name: 'Daily', exact: true })).toHaveClass(/bg-surface text-ink/);
  await reminder.getByRole('button', { name: '30m' }).click();
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect.poll(() => writes.length).toBe(1);
  expect(writes[0].body.recurrence).toMatchObject({ frequency: 'DAILY', reminder_min: 30 });
});

test('saving Edit without touching the repeat keeps the pattern on the server', async ({ page }) => {
  await mockApp(page);
  const writes = await mockRoutines(page);
  await page.goto('/routines');
  await page.getByText('Stretch', { exact: true }).first().click();
  await page.getByPlaceholder("What's the routine?").fill('Stretch more');
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect.poll(() => writes.length).toBe(1);
  expect(writes[0].body).toMatchObject({ title: 'Stretch more', is_recurring: true });
  expect(writes[0].body).not.toHaveProperty('recurrence');
});

test('saving Edit of a routine shorter than the 15 min minimum keeps the pattern too', async ({ page }) => {
  await mockApp(page);
  const writes = await mockRoutines(page, {
    entries: () => [{ ...stretch(), frequency: 'WEEKLY', duration_min: 10 }],
  });
  await page.goto('/routines');
  await page.getByText('Stretch', { exact: true }).first().click();
  await page.getByPlaceholder("What's the routine?").fill('Stretch more');
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect.poll(() => writes.length).toBe(1);
  expect(writes[0].body).not.toHaveProperty('recurrence');
});

const isoOf = (d) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

test('reopening Edit of a weekly routine keeps its weekdays and reminder', async ({ page }) => {
  const now = new Date();
  const todayIdx = (now.getDay() + 6) % 7;
  const otherIdx = todayIdx === 0 ? 2 : 0;
  const other = new Date(now);
  other.setDate(now.getDate() - todayIdx + otherIdx);
  const keys = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];
  const weekly = (date) => ({ ...stretch(), date, frequency: 'WEEKLY', reminder_min: 30 });
  await mockApp(page);
  const writes = await mockRoutines(page, { entries: () => [weekly(today()), weekly(isoOf(other))] });
  await page.goto('/routines');
  await page.getByText('Stretch', { exact: true }).first().click();
  await expect(page.getByRole('button', { name: 'Weekly', exact: true })).toHaveClass(/bg-surface text-ink/);
  const reminder = page.locator('[data-tour="routine.reminder"]');
  await expect(reminder.getByRole('button', { name: '30m' })).toHaveClass(/border-brand/);
  await reminder.getByRole('button', { name: '1h' }).click();
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect.poll(() => writes.length).toBe(1);
  expect(writes[0].body.recurrence).toMatchObject({
    frequency: 'WEEKLY',
    days_of_week: [todayIdx, otherIdx].sort().map((i) => keys[i]),
    reminder_min: 60,
  });
});

test('Edit does not open with partial weekdays when the occurrences fail to load', async ({ page }) => {
  await mockApp(page);
  const writes = await mockRoutines(page, {
    entries: () => [{ ...stretch(), frequency: 'WEEKLY' }],
  });
  await page.goto('/routines');
  await expect(page.getByText('Stretch', { exact: true }).first()).toBeVisible();
  await page.route(/\/api\/daily-routines\/expand\?/, (route) =>
    route.fulfill({ status: 500, json: { message: 'Week failed' } }),
  );
  await page.getByText('Stretch', { exact: true }).first().click();
  await expect(page.getByText('Could not open the routine', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Save changes' })).toHaveCount(0);
  expect(writes).toEqual([]);
});

const daysBetween = (from, to) => (new Date(to) - new Date(from)) / 86400000;

test('duplicating a weekly routine takes its weekdays from several weeks, not just this one', async ({ page }) => {
  const now = new Date();
  const todayIdx = (now.getDay() + 6) % 7;
  const otherIdx = todayIdx === 0 ? 2 : 0;
  const later = new Date(now);
  later.setDate(now.getDate() - todayIdx + 7 + otherIdx);
  const keys = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];
  const weekly = (date) => ({ ...stretch(), date, frequency: 'WEEKLY' });
  await mockApp(page);
  // This week has only today's occurrence; the other weekday shows up in a later week.
  const writes = await mockRoutines(page, {
    entries: (from, to) =>
      from && to && daysBetween(from, to) >= 28 ? [weekly(today()), weekly(isoOf(later))] : [weekly(today())],
  });
  await openMenu(page);
  await page.getByRole('button', { name: 'Duplicate' }).click();
  await expect.poll(() => writes.filter((w) => w.method === 'PUT').length).toBe(1);
  expect(writes.find((w) => w.method === 'PUT').body.recurrence).toMatchObject({
    frequency: 'WEEKLY',
    days_of_week: [todayIdx, otherIdx].sort().map((i) => keys[i]),
  });
});

test('duplicating a one-off routine keeps its date and time and adds no recurrence', async ({ page }) => {
  await mockApp(page);
  const writes = await mockRoutines(page, {
    entries: () => [{ ...stretch(), frequency: 'NONE', reminder_min: null, virtual: false, instance_id: 5 }],
  });
  await openMenu(page);
  await page.getByRole('button', { name: 'Duplicate' }).click();
  await expect.poll(() => writes.filter((w) => w.method === 'PUT').length).toBe(1);
  const put = writes.find((w) => w.method === 'PUT');
  expect(put.body).toMatchObject({
    title: 'Stretch (copy)',
    is_recurring: false,
    scheduled_date: today(),
    scheduled_time: { hours: 8, minutes: 15 },
  });
  expect(put.body.recurrence).toBeUndefined();
});

test('a duplicate whose details fail to save shows a toast and reloads the list so the copy shows', async ({ page }) => {
  await mockApp(page);
  await mockRoutines(page, { putStatus: 500 });
  await openMenu(page);
  const reloads = [];
  page.on('request', (r) => {
    if (/\/api\/daily-routines\/expand\?/.test(r.url())) reloads.push(r.url());
  });
  await page.getByRole('button', { name: 'Duplicate' }).click();
  await expect(page.getByText('Could not duplicate the routine', { exact: true })).toBeVisible();
  await expect.poll(() => reloads.length).toBeGreaterThan(0);
});

test('on a phone the row More button opens Duplicate and Convert', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockApp(page);
  await mockRoutines(page, {
    entries: () => [{ ...stretch(), title: 'A very long routine title that has to truncate on a phone' }],
  });
  await page.goto('/routines');
  const more = page.getByRole('button', { name: 'More' }).first();
  await expect(more).toBeVisible();
  const fits = await more.evaluate((button) => {
    const row = button.parentElement;
    const r = row.getBoundingClientRect();
    const b = button.getBoundingClientRect();
    return b.right <= r.right && b.left >= r.left && row.scrollWidth <= row.clientWidth;
  });
  expect(fits).toBe(true);
  await more.click();
  await expect(page.getByRole('button', { name: 'Duplicate' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Convert to task' })).toBeVisible();
});

test('Duplicate writes nothing when the occurrences fail to load', async ({ page }) => {
  await mockApp(page);
  const writes = await mockRoutines(page, {
    entries: () => [{ ...stretch(), frequency: 'WEEKLY' }],
  });
  await page.goto('/routines');
  await expect(page.getByText('Stretch', { exact: true }).first()).toBeVisible();
  await page.route(/\/api\/daily-routines\/expand\?/, (route) =>
    route.fulfill({ status: 500, json: { message: 'Week failed' } }),
  );
  await page.getByText('Stretch', { exact: true }).first().click({ button: 'right' });
  await page.getByRole('button', { name: 'Duplicate' }).click();
  await expect(page.getByText('Could not duplicate the routine', { exact: true })).toBeVisible();
  expect(writes).toEqual([]);
});

test('convert from the menu closes the modal and offers to open the task', async ({ page }) => {
  await mockApp(page);
  const writes = await mockRoutines(page);
  await openMenu(page);
  await page.getByRole('button', { name: 'Convert to task' }).click();
  const modal = page.getByRole('dialog', { name: 'Convert to task' });
  await expect(modal).toBeVisible();
  await expect(modal.getByRole('combobox', { name: 'Workspace' })).toHaveValue('1');
  await modal.getByRole('button', { name: 'Convert', exact: true }).click();

  await expect(page.getByRole('alertdialog')).toContainText('Open the task now?');
  await expect(modal).toBeHidden();
  expect(writes.find((w) => w.path === 'tasks/31/convert').body).toMatchObject({ workspace_id: 1 });
});

test('a failed convert keeps the modal open and shows a toast', async ({ page }) => {
  await mockApp(page);
  await mockRoutines(page, { convertStatus: 422 });
  await openMenu(page);
  await page.getByRole('button', { name: 'Convert to task' }).click();
  const modal = page.getByRole('dialog', { name: 'Convert to task' });
  await modal.getByRole('button', { name: 'Convert', exact: true }).click();

  await expect(page.getByText('Could not convert the routine', { exact: true })).toBeVisible();
  await expect(page.getByText('Conversion exploded', { exact: true })).toBeVisible();
  await expect(modal).toBeVisible();
  await expect(page.getByRole('alertdialog')).toHaveCount(0);
});

test('switching workspace fast never lists the previous workspace categories', async ({ page }) => {
  await mockApp(page);
  await page.route('**/api/workspaces', (route) =>
    route.fulfill({
      json: {
        data: [
          { id: 1, name: 'Demo', code: 'demo', user_id: 1, is_default: true },
          { id: 2, name: 'Side', code: 'side', user_id: 1, is_default: false },
        ],
      },
    }),
  );
  await page.route(/\/api\/project_categories\?/, async (route) => {
    const workspaceId = new URL(route.request().url()).searchParams.get('workspace_id');
    if (workspaceId === '1') await new Promise((resolve) => setTimeout(resolve, 1500));
    const title = workspaceId === '1' ? 'Demo category' : 'Side category';
    await route.fulfill({ json: { data: [{ id: Number(workspaceId) * 10, title }] } });
  });
  await mockRoutines(page);
  await openMenu(page);
  await page.getByRole('button', { name: 'Convert to task' }).click();
  const modal = page.getByRole('dialog', { name: 'Convert to task' });
  const category = modal.getByRole('combobox', { name: 'Category' });
  await modal.getByRole('combobox', { name: 'Workspace' }).selectOption('2');
  await expect(category.locator('option', { hasText: 'Side category' })).toHaveCount(1);
  await page.waitForTimeout(2000);
  await expect(category.locator('option', { hasText: 'Demo category' })).toHaveCount(0);
  await expect(category.locator('option', { hasText: 'Side category' })).toHaveCount(1);
});
