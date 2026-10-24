import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

// TM-703: returning to Daily Routines renders the last routines at once, without skeletons.
const DELAY = 1500;
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const todayIso = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(
    2,
    '0',
  )}-${String(d.getDate()).padStart(2, '0')}`;
};
const entry = (taskId, title, completed = false) => ({
  task_id: taskId,
  title,
  description: null,
  routine_category: { id: 'health', name: 'Health', color: '#22c55e' },
  date: todayIso(),
  time: '09:00',
  duration_min: 30,
  reminder_min: null,
  completed,
  status: completed ? 'completed' : null,
  instance_id: completed ? 501 : null,
  virtual: !completed,
  frequency: 'DAILY',
  scheduled_for: null,
});

const setup = async (page, view = 'list') => {
  const fixture = {
    taskId: 11,
    title: 'Morning stretch',
    empty: false,
    fail: false,
    completed: false,
    doneToday: 1,
    inFlight: 0,
    // `completed` as each expand response saw it when its request arrived.
    responses: [],
  };
  await page.addInitScript((v) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr.dailyRoutines.view', v);
  }, view);
  await mockApp(page);
  await page.route('**/api/daily-routines/**', async (r) => {
    if (r.request().method() !== 'GET') return r.fallback();
    const { pathname } = new URL(r.request().url());
    if (pathname.endsWith('/expand/stats')) {
      const done = fixture.doneToday;
      await wait(DELAY);
      return r.fulfill({
        json: { data: { [todayIso()]: { fires: 2, completed: done } } },
      });
    }
    if (!pathname.endsWith('/expand')) return r.fulfill({ json: { data: [] } });
    const completed = fixture.completed;
    fixture.inFlight++;
    await wait(DELAY);
    fixture.inFlight--;
    fixture.responses.push(completed);
    if (fixture.fail)
      return r.fulfill({ status: 400, json: { message: 'Routines down' } });
    return r.fulfill({
      json: {
        data: fixture.empty
          ? []
          : [entry(fixture.taskId, fixture.title, completed)],
      },
    });
  });
  return fixture;
};

// Records every skeleton that enters the page content, however briefly.
function installFlashWatcher() {
  window.__flashes?.observer.disconnect();
  const found = [];
  const SKELETON = '.animate-pulse.bg-muted';
  const check = (node) => {
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const hits = [
      ...(node.matches(SKELETON) ? [node] : []),
      ...node.querySelectorAll(SKELETON),
    ].filter((el) => el.closest('main'));
    for (const el of hits) found.push(el.outerHTML.slice(0, 120));
  };
  const observer = new MutationObserver((mutations) => {
    for (const m of mutations)
      if (m.type !== 'childList') check(m.target);
      else m.addedNodes.forEach(check);
  });
  observer.observe(document, {
    subtree: true,
    childList: true,
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
const leaveToList = async (page) => {
  await nav(page, 'List');
  await expect(page).toHaveURL(/\/demo\/list/);
  await expect(page.locator('[data-task-id="1"]')).toContainText(
    'Original task',
  );
};
const routine = (page, title) =>
  page.locator('main').getByText(title, { exact: true });
const todayHeader = (page) =>
  page.locator('main [data-dr-kind="day-cell"] > header');
const EMPTY_TODAY = /^\s*Today\s*0\s*$/;
// Shorter than the API delay: content shown this fast came from the last snapshot.
const SEEDED = { timeout: DELAY - 500 };

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
});

test('a return visit renders the last routines without skeletons, then refreshes', async ({
  page,
}) => {
  const fixture = await setup(page);
  await page.addInitScript(installFlashWatcher);

  await page.goto('/routines');
  await expect(routine(page, 'Morning stretch')).toBeVisible();
  expect((await flashes(page)).length).toBeGreaterThan(0);
  await expect(page.locator('main .animate-pulse.bg-muted')).toHaveCount(0);

  await leaveToList(page);
  fixture.taskId = 12;
  fixture.title = 'Evening walk';
  await watchFlashes(page);
  await nav(page, 'Daily Routines');
  await expect(page).toHaveURL(/\/routines$/);
  await expect(routine(page, 'Morning stretch')).toBeVisible(SEEDED);
  await expect(routine(page, 'Evening walk')).toBeVisible();
  await expect(routine(page, 'Morning stretch')).toHaveCount(0);
  await wait(DELAY);
  expect(await flashes(page)).toEqual([]);
});

test('a routine deleted while away never shows on return', async ({ page }) => {
  const fixture = await setup(page);
  await page.route('**/api/daily-routines/tasks/11', (r) => {
    if (r.request().method() !== 'DELETE') return r.fallback();
    fixture.taskId = 12;
    fixture.title = 'Evening walk';
    return r.fulfill({ json: { data: null } });
  });
  await page.goto('/routines');
  await expect(routine(page, 'Morning stretch')).toBeVisible();
  await leaveToList(page);

  await page.evaluate(async () => {
    const { deleteDailyTask } =
      await import('/src/actions/tmgr/daily-tasks.ts');
    await deleteDailyTask(11);
    window.__deletedSeen = false;
    const tick = () => {
      if (
        document.querySelector('main')?.textContent.includes('Morning stretch')
      )
        window.__deletedSeen = true;
      requestAnimationFrame(tick);
    };
    tick();
  });
  await nav(page, 'Daily Routines');
  await expect(routine(page, 'Evening walk')).toBeVisible();
  expect(await page.evaluate(() => window.__deletedSeen)).toBe(false);
});

test('a failed refresh on a return visit keeps the last routines and shows the error', async ({
  page,
}) => {
  const fixture = await setup(page);
  await page.goto('/routines');
  await expect(routine(page, 'Morning stretch')).toBeVisible();
  await leaveToList(page);

  fixture.fail = true;
  await nav(page, 'Daily Routines');
  await expect(routine(page, 'Morning stretch')).toBeVisible(SEEDED);
  const alert = page.locator('main [role="alert"]');
  await expect(alert).toBeVisible({ timeout: DELAY * 3 });
  await expect(alert.getByRole('button', { name: 'Try again' })).toBeVisible();
  await expect(routine(page, 'Morning stretch')).toBeVisible();
  await expect(page.locator('main .animate-pulse.bg-muted')).toHaveCount(0);
});

test('an empty routines list returns to its empty view without skeletons', async ({
  page,
}) => {
  const fixture = await setup(page);
  fixture.empty = true;
  await page.goto('/routines');
  await expect(todayHeader(page)).toHaveText(EMPTY_TODAY);
  await leaveToList(page);

  await watchFlashes(page);
  await nav(page, 'Daily Routines');
  await expect(todayHeader(page)).toHaveText(EMPTY_TODAY, SEEDED);
  await wait(DELAY);
  await expect(todayHeader(page)).toHaveText(EMPTY_TODAY);
  expect(await flashes(page)).toEqual([]);
});

test('logout drops the routines snapshots', async ({ page }) => {
  await setup(page);
  await page.goto('/routines');
  await expect(routine(page, 'Morning stretch')).toBeVisible();
  await leaveToList(page);

  const snapshots = await page.evaluate(async () => {
    const { requestCache } = await import('/src/utils/requestCache.ts');
    const { default: store } = await import('/src/store/index.js');
    const keys = () =>
      [...requestCache['cache'].keys()].filter((k) =>
        k.includes('routines-snapshot'),
      );
    const before = keys().length;
    await store.dispatch('logout');
    return { before, after: keys() };
  });
  expect(snapshots.before).toBeGreaterThan(0);
  expect(snapshots.after).toEqual([]);
});

test('a routine ticked while the return refresh is in flight stays ticked', async ({
  page,
}) => {
  const fixture = await setup(page);
  await page.route('**/api/daily-routines/tasks/11/complete-on', (r) => {
    fixture.completed = true;
    return r.fulfill({
      json: {
        data: {
          instance_id: 501,
          task_id: 11,
          date: todayIso(),
          status: 'completed',
          completed: true,
        },
      },
    });
  });
  await page.goto('/routines');
  await expect(routine(page, 'Morning stretch')).toBeVisible();
  await leaveToList(page);

  await watchFlashes(page);
  await nav(page, 'Daily Routines');
  const check = page.locator('main [data-tour="routines.instance"]');
  await expect(check).toBeVisible(SEEDED);
  expect(fixture.inFlight).toBe(1);
  await check.click();
  await expect(check).toHaveClass(/bg-status-done/);
  expect(fixture.inFlight).toBeGreaterThan(0);
  await page.evaluate(() => {
    window.__unticked = false;
    const tick = () => {
      const el = document.querySelector('main [data-tour="routines.instance"]');
      if (!el || !el.className.includes('bg-status-done'))
        window.__unticked = true;
      requestAnimationFrame(tick);
    };
    tick();
  });

  // Cold load, the stale return refresh (sent before the tick), then the follow-up refresh.
  await expect
    .poll(() => fixture.responses, { timeout: DELAY * 4 })
    .toEqual([false, false, true]);
  await wait(500);
  await expect(check).toHaveClass(/bg-status-done/);
  expect(await page.evaluate(() => window.__unticked)).toBe(false);
  expect(await flashes(page)).toEqual([]);
});

test('a year-view return visit renders the last stats without skeletons', async ({
  page,
}) => {
  const fixture = await setup(page, 'year');
  const todayCell = (done) =>
    page.locator(`main [title="${new Date().toDateString()} — ${done}/2"]`);
  await page.addInitScript(installFlashWatcher);
  await page.goto('/routines');
  await expect(todayCell(1)).toBeVisible();
  expect((await flashes(page)).length).toBeGreaterThan(0);
  await leaveToList(page);

  fixture.doneToday = 2;
  await watchFlashes(page);
  await nav(page, 'Daily Routines');
  await expect(todayCell(1)).toBeVisible(SEEDED);
  await expect(todayCell(2)).toBeVisible();
  await wait(DELAY);
  expect(await flashes(page)).toEqual([]);
});
