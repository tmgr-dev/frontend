import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const ENGINE = process.env.TOURS_E2E_ENGINE === '1';
const engine = ENGINE ? test : test.skip;
const SHOTS = 'test-results/tours';

const SIZES = [
  { name: 'desktop', width: 1280, height: 800 },
  { name: 'phone', width: 390, height: 844 },
];
const SCHEMES = ['light', 'dark'];

const freshState = () => ({
  intent: null,
  tips_mode: 'auto',
  hub_hidden: false,
  checklist_hidden: false,
  tours_generation: 0,
  tours: {},
  facts: {},
});

const mockOnboarding = async (page, seed = {}) => {
  const state = { ...freshState(), ...seed };
  const writes = [];
  await page.route('**/api/onboarding**', async (route) => {
    const request = route.request();
    const method = request.method();
    const path = new URL(request.url()).pathname.replace(/^.*\/api\//, '');
    const body = method === 'GET' || method === 'DELETE' ? null : request.postDataJSON();
    if (method !== 'GET') writes.push({ method, path, body });
    if (path === 'onboarding/preferences' && method === 'PUT') {
      for (const key of ['tips_mode', 'hub_hidden', 'checklist_hidden']) {
        if (body?.[key] !== undefined) state[key] = body[key];
      }
    } else if (path.startsWith('onboarding/tours/') && method === 'PUT') {
      const id = decodeURIComponent(path.slice('onboarding/tours/'.length));
      if ((body.generation ?? 0) >= state.tours_generation) {
        state.tours[id] = {
          status: body.status,
          step_id: body.step_id ?? null,
          version: body.version ?? 1,
          generation: body.generation ?? 0,
        };
      }
    } else if (path === 'onboarding/tours' && method === 'DELETE') {
      state.tours = {};
      state.tours_generation += 1;
    } else if (path === 'onboarding/intent' && method === 'POST') {
      state.intent = body.intent;
    }
    const data = path === 'onboarding/facts' ? {} : state;
    await route.fulfill({ json: { data } });
  });
  return { state, writes };
};

const baseTask = {
  description: null,
  description_json: null,
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
  created_at: '2026-10-12T10:00:00Z',
  updated_at: '2026-10-12T10:00:00Z',
};

const mockTasks = async (page) => {
  const created = [];
  await page.route('**/api/tasks', async (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    const payload = route.request().postDataJSON();
    const task = {
      ...baseTask,
      id: 50 + created.length,
      title: payload.title ?? 'New task',
    };
    created.push(task);
    await route.fulfill({ json: { data: task } });
  });
  await page.route('**/api/tasks/current**', async (route) => {
    const rows = [...created, { ...baseTask, id: 1, title: 'Original task' }];
    await route.fulfill({
      json: {
        data: rows,
        meta: { current_page: 1, per_page: 10, total: rows.length, last_page: 1, from: 1, to: rows.length },
      },
    });
  });
  return created;
};

const setScheme = async (page, scheme) => {
  await page.emulateMedia({ colorScheme: scheme });
  await page.evaluate(
    (dark) => document.documentElement.classList.toggle('dark', dark),
    scheme === 'dark',
  );
  await page.waitForTimeout(300);
};

const shoot = async (page, name, size, scheme) => {
  await setScheme(page, scheme);
  await page.screenshot({ path: `${SHOTS}/${name}-${size.name}-${scheme}.png` });
};

const boot = async (page, size, { onboarding = { tips_mode: 'hub_only' }, ...options } = {}) => {
  await page.setViewportSize({ width: size.width, height: size.height });
  await mockApp(page, { multiStatus: true, ...options });
  const created = await mockTasks(page);
  const fixture = await mockOnboarding(page, onboarding);
  return { ...fixture, created };
};

const popover = (page) => page.locator('[role="dialog"][aria-labelledby^="tour-"]').first();

test.describe('anchors and hub (need the tour engine from W1 for useTours)', () => {
  test('every anchor of the S2 catalog is in the DOM on its page', async ({ page }) => {
    await boot(page, SIZES[0]);
    await page.goto('/demo/list');
    for (const anchor of [
      'nav.sidebar',
      'nav.workspace-switcher',
      'nav.user-menu',
      'nav.archive',
      'nav.getting-started',
      'nav.menu-button',
      'nav.create-task',
      'list.row',
    ]) {
      await expect(page.locator(`[data-tour="${anchor}"]`).first(), anchor).toBeAttached();
    }
    await page.locator('[data-task-id="1"]').first().click();
    for (const anchor of [
      'task-form.title',
      'task-form.status',
      'task-form.timer',
      'task-form.description',
      'task-form.comments',
    ]) {
      await expect(page.locator(`[data-tour="${anchor}"]`).first(), anchor).toBeAttached();
    }
  });

  test('board anchors, with a 44px touch area on the drag handle', async ({ page }) => {
    await boot(page, SIZES[0]);
    await page.goto('/demo/board');
    await expect(page.locator('[data-tour="board.column"]').first()).toBeAttached();
    const add = page.locator('[data-tour="board.add-in-column"]').first();
    await expect(add).toBeAttached();
    await add.focus();
    await expect(add).toHaveCSS('opacity', '1');
    const handle = page.locator('[data-tour="board.card-handle"]').first();
    await expect(handle).toHaveAttribute('aria-label', 'Drag to move');
    const area = await handle.evaluate((el) => {
      const style = getComputedStyle(el, '::after');
      return { width: parseFloat(style.width), height: parseFloat(style.height) };
    });
    expect(area.width).toBeGreaterThanOrEqual(44);
    expect(area.height).toBeGreaterThanOrEqual(44);
  });

  for (const size of SIZES) {
    test(`the hub lists tours and keeps its controls working at ${size.name}`, async ({ page }) => {
      const { writes } = await boot(page, size, {
        onboarding: {
          tips_mode: 'hub_only',
          tours: { 'start.nav': { status: 'completed', step_id: null, version: 1, generation: 0 } },
        },
      });
      await page.goto('/demo/getting-started');
      await expect(page.getByRole('heading', { name: 'Getting started', level: 1 })).toBeVisible();
      for (const anchor of ['hub.catalog', 'hub.tips-toggle', 'hub.checklist']) {
        await expect(page.locator(`[data-tour="${anchor}"]`)).toBeVisible();
      }
      await expect(page.getByText('Find your way around')).toBeVisible();
      await expect(page.getByText('Done', { exact: true }).first()).toBeVisible();
      await expect(page.getByTestId('hub-start-start.first-task')).toBeVisible();
      for (const scheme of SCHEMES) await shoot(page, 'hub', size, scheme);

      await page.getByRole('switch', { name: 'Show tips automatically' }).click();
      await expect
        .poll(() => writes.find((w) => w.path === 'onboarding/preferences')?.body)
        .toEqual({ tips_mode: 'auto' });

      await page.getByTestId('hub-hide').click();
      await expect
        .poll(() => writes.filter((w) => w.path === 'onboarding/preferences').at(-1)?.body)
        .toEqual({ hub_hidden: true });
    });
  }

  test('the help menu opens the hub and tips settings', async ({ page }) => {
    await boot(page, SIZES[0]);
    await page.goto('/demo/list');
    await page.getByRole('button', { name: 'Help and tours' }).click();
    await page.getByRole('menuitem', { name: 'Getting started' }).click();
    await expect(page).toHaveURL(/\/demo\/getting-started$/);
    await page.getByRole('button', { name: 'Help and tours' }).click();
    await page.getByRole('menuitem', { name: 'Tips settings' }).click();
    await expect(page).toHaveURL(/\/settings\?tab=about/);
    await expect(page.getByText('Tips and tours')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Reset all tours' })).toBeVisible();
  });

  test('tips settings change the mode and reset all tours', async ({ page }) => {
    const { writes } = await boot(page, SIZES[0]);
    await page.goto('/settings?tab=about');
    await page.getByLabel('Never show tips').check();
    await expect
      .poll(() => writes.find((w) => w.path === 'onboarding/preferences')?.body)
      .toEqual({ tips_mode: 'off' });
    await page.getByRole('button', { name: 'Reset all tours' }).click();
    await page.getByTestId('tips-reset-confirm').click();
    await expect.poll(() => writes.some((w) => w.method === 'DELETE' && w.path === 'onboarding/tours')).toBe(true);
  });
});

test.describe('G2 and TI1 with the real engine (TOURS_E2E_ENGINE=1, needs W1 and W2 merged)', () => {
  for (const size of SIZES) {
    for (const scheme of SCHEMES) {
      engine(`G2 create your first task at ${size.name}, ${scheme}`, async ({ page }) => {
        const { writes, created } = await boot(page, size);
        await page.goto('/demo/getting-started');
        await page.getByTestId('hub-start-start.first-task').click();
        await setScheme(page, scheme);
        await expect(popover(page)).toContainText('Click here to create a task');
        await shoot(page, 'g2-step1', size, scheme);
        if (size.width < 640) {
          const box = await popover(page).boundingBox();
          expect(box.y + box.height).toBeGreaterThan(size.height - 40);
          for (const button of await popover(page).getByRole('button').all()) {
            const target = await button.boundingBox();
            expect(target.height).toBeGreaterThanOrEqual(44);
          }
        }

        await page.locator('[data-tour="nav.create-task"]').first().click();
        await expect(popover(page)).toContainText('Ctrl+S');
        await shoot(page, 'g2-step2', size, scheme);

        await page.locator('[data-tour="task-form.title"]').fill('First task from the tour');
        await page.keyboard.press('Control+s');
        await expect.poll(() => created.length).toBe(1);
        expect(writes.some((w) => w.path === 'onboarding/tours/start.first-task')).toBe(true);
        await expect(popover(page)).toContainText('Done. Click a task');
        await page.waitForTimeout(1500); await page.screenshot({ path: '/private/tmp/claude-502/-Users-iuriikarpov-projects-own-projects-tmgr-dev/f84a97c2-7084-46b5-96ab-f0a34c476b3a/scratchpad/g2dbg.png' });
        await expect(page.locator(`[data-task-id="${created[0].id}"]`).first()).toBeVisible();
        await shoot(page, 'g2-step3', size, scheme);
      });

      engine(`TI1 run a timer at ${size.name}, ${scheme}`, async ({ page }) => {
        const { writes } = await boot(page, size);
        await page.route('**/api/tasks/1/countdown', (route) =>
          route.fulfill({ json: { data: { id: 1, start_time: 1 } } }),
        );
        await page.goto('/demo/getting-started');
        await page.getByTestId('hub-start-time.timer').click();
        await expect(popover(page).getByRole('button', { name: /take me there/i })).toBeVisible();
        await shoot(page, 'ti1-take-me', size, scheme);
        await popover(page).getByRole('button', { name: /take me there/i }).click();
        await page.locator('[data-task-id="1"]').first().click();
        await expect(popover(page)).toContainText('Press play');
        await shoot(page, 'ti1-step1', size, scheme);
        await page.locator('[data-tour="task-form.timer"] button').first().click();
        await expect.poll(() => writes.some((w) => w.path === 'onboarding/tours/time.timer')).toBe(true);
        await shoot(page, 'ti1-step2', size, scheme);
      });
    }
  }

  engine('Esc pauses the tour, keeps the task panel open and the hub offers Resume', async ({ page }) => {
    const { writes } = await boot(page, SIZES[0]);
    await page.goto('/demo/getting-started');
    await page.getByTestId('hub-start-start.first-task').click();
    await page.locator('[data-tour="nav.create-task"]').first().click();
    await expect(popover(page)).toContainText('Ctrl+S');
    await page.keyboard.press('Escape');
    await expect(popover(page)).toBeHidden();
    await expect(page.locator('[data-tour="task-form.title"]').first()).toBeVisible();
    await expect
      .poll(() => writes.filter((w) => w.path === 'onboarding/tours/start.first-task').at(-1)?.body)
      .toMatchObject({ status: 'paused', step_id: 'title' });
    await page.goto('/demo/getting-started');
    const row = page.locator('[data-tour-id="start.first-task"]');
    await expect(row.getByRole('button', { name: /Resume/ })).toBeVisible();
    await expect(row).toContainText('step 2 of 3');
  });

  engine('G1 runs from the keyboard alone and focus stays inside the popover', async ({ page }) => {
    await boot(page, SIZES[0]);
    await page.goto('/demo/getting-started');
    await page.getByTestId('hub-start-start.nav').focus();
    await page.keyboard.press('Enter');
    await expect(popover(page)).toBeVisible();
    await expect
      .poll(() => page.evaluate(() => !!document.activeElement?.closest('[role="dialog"]')))
      .toBe(true);
    for (let i = 0; i < 8; i++) {
      await page.keyboard.press('Tab');
      expect(await page.evaluate(() => !!document.activeElement?.closest('[role="dialog"]'))).toBe(true);
    }
    await page.keyboard.press('Escape');
    await expect(popover(page)).toBeHidden();
  });
});
