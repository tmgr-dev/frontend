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

const mockRoutines = async (page, { entries = () => [stretch()] } = {}) => {
  const writes = [];
  // routines.first auto-offers 600ms after landing on /routines and covers the page.
  await page.addInitScript(() => sessionStorage.setItem('tmgr:tours:auto', 'true'));
  await page.route('**/api/daily-routines/**', (route) => {
    const request = route.request();
    const method = request.method();
    const path = new URL(request.url()).pathname.replace(/^.*\/api\/daily-routines\//, '');
    if (method !== 'GET') writes.push({ method, path, body: request.postDataJSON() });
    if (method === 'PUT' && /^tasks\/\d+$/.test(path))
      return route.fulfill({ json: { data: { id: Number(path.split('/')[1]) } } });
    if (path.startsWith('expand/stats')) return route.fulfill({ json: { data: {} } });
    if (path.startsWith('expand')) return route.fulfill({ json: { data: entries() } });
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
