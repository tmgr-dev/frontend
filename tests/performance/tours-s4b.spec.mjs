import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';
import { SCHEMES, SIZES, mockOnboarding, setScheme, shoot } from './toursHelpers.mjs';
import { mockGraph, mockMisc, mockModules, mockPages, mockRoutines } from './toursS4bMocks.mjs';

const e2e = process.env.S4_E2E === '1' ? test : test.skip;

const boot = async (page, size, { graph = true, timers = false, canManage = true, modules } = {}) => {
  await page.setViewportSize({ width: size.width, height: size.height });
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await mockApp(page, { multiStatus: true });
  const fixture = await mockOnboarding(page, { tips_mode: 'hub_only' });
  const calls = [];
  const state = await mockModules(page, { graph, canManage, only: modules });
  await mockPages(page, calls);
  await mockRoutines(page, calls);
  await mockGraph(page);
  await mockMisc(page, calls, { timers });
  return { ...fixture, calls, errors, modules: state };
};

const popover = (page) => page.locator('[role="dialog"][aria-labelledby^="tour-"]').first();
const lastWrite = (writes, id) => writes.filter((w) => w.path === `onboarding/tours/${id}`).at(-1)?.body;
const hubStart = (page, id) => page.getByTestId(`hub-start-${id}`);
const primary = (page, name) => popover(page).getByRole('button', { name, exact: true });

const skipStep = async (page, mobile) => {
  if (mobile) {
    await popover(page).getByRole('button', { name: 'Skip step' }).click();
  } else {
    await popover(page).getByRole('button', { name: /^Skip$/ }).click();
    await page.getByRole('menuitem', { name: /Skip this step/ }).click();
  }
};

const attached = async (page, anchors) => {
  for (const anchor of anchors) {
    await expect(page.locator(`[data-tour="${anchor}"]`).first(), anchor).toBeAttached();
  }
};

const startFromHub = async (page, id) => {
  await page.goto('/demo/getting-started');
  await hubStart(page, id).click();
  await expect(popover(page)).toBeVisible();
};

const expectClear = async (page, anchor) => {
  const target = page.locator(`[data-tour="${anchor}"]`).first();
  await expect(target).toBeInViewport();
  await page.waitForTimeout(900);
  const a = await target.boundingBox();
  const p = await popover(page).boundingBox();
  const vw = page.viewportSize();
  const x1 = Math.max(a.x, 0);
  const y1 = Math.max(a.y, 0);
  const x2 = Math.min(a.x + a.width, vw.width);
  const y2 = Math.min(a.y + a.height, vw.height);
  const visible = Math.max(0, x2 - x1) * Math.max(0, y2 - y1);
  const ox = Math.max(0, Math.min(x2, p.x + p.width) - Math.max(x1, p.x));
  const oy = Math.max(0, Math.min(y2, p.y + p.height) - Math.max(y1, p.y));
  expect(ox * oy, `${anchor} is mostly covered by the tour card`).toBeLessThan(visible * 0.6);
  expect(p.x, `${anchor} card is on screen`).toBeGreaterThanOrEqual(-1);
  expect(p.y, `${anchor} card is on screen`).toBeGreaterThanOrEqual(-1);
  expect(p.x + p.width).toBeLessThanOrEqual(vw.width + 1);
  expect(p.y + p.height).toBeLessThanOrEqual(vw.height + 1);
};

const shots = async (page, name, size) => {
  for (const scheme of SCHEMES) await shoot(page, name, size, scheme);
  await setScheme(page, 'light');
};

const openFirstTask = async (page) => {
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
};

const TOURS = [
  { id: 'tasks.statuses', mobile: false, url: /\/demo\/board$/, anchor: 'statuses.editor' },
  { id: 'tasks.categories', mobile: true, take: 'categories.create' },
  { id: 'tasks.relations', mobile: false, take: 'task-form.relations' },
  { id: 'tasks.files-page', mobile: true, url: /\/demo\/files$/, anchor: 'files.list' },
  { id: 'tasks.multi-select', mobile: false, url: /\/demo\/list$/, anchor: 'list.select' },
  { id: 'time.parallel', mobile: false, intro: true },
  { id: 'time.checkpoints', mobile: false, take: 'task-form.checkpoints' },
  { id: 'time.pomodoro', mobile: true, take: 'task-form.pomodoro' },
  { id: 'time.export', mobile: false, url: /\/demo\/list$/, anchor: 'list.select' },
  { id: 'pages.first', mobile: true, take: 'pages.new' },
  { id: 'pages.context', mobile: false, anchor: 'pages.type-picker' },
  { id: 'pages.links', mobile: false, take: 'page.editor' },
  { id: 'pages.versions', mobile: false, take: 'page.versions' },
  { id: 'pages.people-meetings', mobile: false, anchor: 'pages.type-picker' },
  { id: 'pages.import', mobile: false, url: /\/demo\/pages$/, anchor: 'pages.import' },
  { id: 'pages.task-from-selection', mobile: false, take: 'page.editor' },
  { id: 'graph.map', mobile: false, url: /\/demo\/map$/, anchor: 'map.canvas' },
  { id: 'graph.enable', mobile: false, url: /\/settings\/modules$/, anchor: 'modules.graph', graphOff: true },
  { id: 'routines.first', mobile: true, take: 'routines.add', mobileTake: 'routines.quick-add' },
  { id: 'routines.views', mobile: true, url: /\/routines$/, anchor: 'routines.view-switcher' },
  { id: 'routines.reminders', mobile: false, take: 'routines.instance' },
];

for (const size of SIZES) {
  const mobile = size.width < 640;

  e2e(`hub lists every S4b tour as startable with all modules on at ${size.name}`, async ({ page }) => {
    const { errors } = await boot(page, size);
    await page.goto('/demo/getting-started');
    await expect(page.locator('[data-tour-id="graph.enable"]')).toHaveCount(0);
    for (const tour of TOURS) {
      if (tour.graphOff) continue;
      const start = hubStart(page, tour.id);
      if (mobile && !tour.mobile) await expect(start, `${tour.id} unavailable: platform`).toHaveCount(0);
      else await expect(start, tour.id).toBeVisible();
    }
    for (const scheme of SCHEMES) await shoot(page, 'hub-s4b', size, scheme);
    expect(errors).toEqual([]);
  });

  e2e(`hub explains module-off tours and hides owner-only ones from members at ${size.name}`, async ({ page }) => {
    const { errors } = await boot(page, size, { canManage: false, modules: ['board', 'categories'] });
    await page.goto('/demo/getting-started');
    await expect(hubStart(page, 'tasks.categories')).toBeVisible();
    if (!mobile) {
      await expect(page.locator('[data-tour-id="graph.map"]')).toContainText('Ask the workspace owner to turn on Graph');
      await expect(page.locator('[data-tour-id="pages.first"]')).toContainText('Ask the workspace owner to turn on Pages');
      await expect(page.locator('[data-tour-id="time.checkpoints"]')).toContainText('Ask the workspace owner');
      await expect(page.locator('[data-tour-id="graph.enable"]')).toHaveCount(0);
      await expect(page.locator('[data-tour-id="tasks.statuses"]')).toHaveCount(0);
      await expect(hubStart(page, 'graph.map')).toHaveCount(0);
    }
    await expect(page.locator('[data-tour-id="routines.first"]')).toContainText('Ask the workspace owner');
    for (const scheme of SCHEMES) await shoot(page, 'hub-s4b-members-modules-off', size, scheme);
    expect(errors).toEqual([]);
  });

  e2e(`owner sees the turn-on message for Graph and Pages at ${size.name}`, async ({ page }) => {
    test.skip(mobile, 'graph is web and desktop only');
    const { errors } = await boot(page, size, { modules: ['board'] });
    await page.goto('/demo/getting-started');
    await expect(page.locator('[data-tour-id="graph.map"]')).toContainText('Turn on Graph in Settings -> Modules');
    await expect(page.locator('[data-tour-id="pages.first"]')).toContainText('Turn on Pages in Settings -> Modules');
    await expect(page.locator('[data-tour-id="time.export"]')).toContainText('Turn on Exports');
    await expect(hubStart(page, 'tasks.statuses')).toHaveCount(0);
    for (const scheme of SCHEMES) await shoot(page, 'hub-s4b-modules-off', size, scheme);
    expect(errors).toEqual([]);
  });

  for (const tour of TOURS) {
    if (mobile && !tour.mobile) continue;
    e2e(`${tour.id} starts from the hub and shows a useful first step at ${size.name}`, async ({ page }) => {
      const { errors } = await boot(page, size, { graph: !tour.graphOff, timers: tour.id.startsWith('time.') });
      await startFromHub(page, tour.id);
      if (tour.url) await expect(page).toHaveURL(tour.url);
      if (tour.take) {
        await expect(popover(page)).toContainText('could not find this');
        await expect(primary(page, 'Take me there')).toBeVisible();
        const route = page.url();
        await primary(page, 'Take me there').click();
        await expect(page).not.toHaveURL(route);
        await expect(popover(page)).toBeVisible();
      } else if (tour.anchor) {
        const anchor = mobile && tour.mobileAnchor ? tour.mobileAnchor : tour.anchor;
        await expect(popover(page)).not.toContainText('could not find this');
        await expect(page.locator(`[data-tour="${anchor}"]`).first()).toBeVisible();
        await expectClear(page, anchor);
      } else if (tour.intro) {
        await expect(popover(page)).toContainText('Several timers');
      }
      await shots(page, tour.id, size);
      await page.keyboard.press('Escape');
      expect(errors).toEqual([]);
    });
  }
}

const driveTour = (name, run, { phone = true } = {}) => {
  for (const size of SIZES) {
    const mobile = size.width < 640;
    e2e(`${name} at ${size.name}`, async ({ page }) => {
      test.skip(mobile && !phone, 'web and desktop only');
      await run({ page, size, mobile });
    });
  }
};

driveTour('tasks.categories: create a category, tip on the row, done', async ({ page, size }) => {
  const { writes, calls, errors } = await boot(page, size);
  await startFromHub(page, 'tasks.categories');
  await primary(page, 'Take me there').click();
  await expect(page).toHaveURL(/\/demo\/categories$/);
  await attached(page, ['categories.create', 'categories.row']);
  await expectClear(page, 'categories.create');
  await shots(page, 'tasks.categories-s1', size);
  await page.locator('[data-tour="categories.create"]').first().click();
  await expect(page).toHaveURL(/\/demo\/categories\/create$/);
  await page.getByPlaceholder('Name').fill('Fixture');
  await page.getByPlaceholder('e.g., PROJ, FEAT, BUG').fill('FIX');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect.poll(() => calls.includes('POST project_categories')).toBe(true);
  await expect.poll(() => lastWrite(writes, 'tasks.categories')?.step_id).toBe('s2');
  await expect(page).toHaveURL(/\/demo\/categories$/);
  await expect(popover(page)).toContainText('Open a category');
  await expectClear(page, 'categories.row');
  await shots(page, 'tasks.categories-s2', size);
  await primary(page, 'Done').click();
  await expect.poll(() => lastWrite(writes, 'tasks.categories')?.status).toBe('completed');
  expect(errors).toEqual([]);
});

driveTour('tasks.categories: with no row to point at the optional tip is skipped and the tour completes', async ({ page, size }) => {
  const { writes, calls, errors } = await boot(page, size);
  await page.route('**/api/project_categories/children**', (route) =>
    route.fulfill({ json: { data: [], meta: { current_page: 1, per_page: 15, last_page: 1, total: 0 }, links: {} } }),
  );
  await startFromHub(page, 'tasks.categories');
  await primary(page, 'Take me there').click();
  await expect(page).toHaveURL(/\/demo\/categories$/);
  await expect(popover(page)).toContainText('Create a category');
  await shots(page, 'tasks.categories-empty', size);
  await page.locator('[data-tour="categories.create"]').first().click();
  await page.getByPlaceholder('Name').fill('Fixture');
  await page.getByPlaceholder('e.g., PROJ, FEAT, BUG').fill('FIX');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect.poll(() => calls.includes('POST project_categories')).toBe(true);
  await expect.poll(() => lastWrite(writes, 'tasks.categories')?.status, { timeout: 15000 }).toBe('completed');
  expect(errors).toEqual([]);
});

driveTour('tasks.relations: skip step on a wait step completes the tour', async ({ page, size, mobile }) => {
  const { writes, errors } = await boot(page, size);
  await startFromHub(page, 'tasks.relations');
  await skipStep(page, mobile);
  await expect.poll(() => lastWrite(writes, 'tasks.relations')?.status, { timeout: 15000 }).toMatch(/completed|skipped/);
  expect(errors).toEqual([]);
}, { phone: false });

driveTour('tasks.statuses: spot then the optional add-status wait', async ({ page, size }) => {
  const { writes, errors } = await boot(page, size);
  await startFromHub(page, 'tasks.statuses');
  await expect(page).toHaveURL(/\/demo\/board$/);
  await expect(popover(page)).toContainText('Statuses');
  await shots(page, 'tasks.statuses-s1', size);
  await primary(page, 'Next').click();
  await expect.poll(() => lastWrite(writes, 'tasks.statuses')?.step_id).toBe('s2');
  await expect(popover(page)).toContainText('Add a status');
  await shots(page, 'tasks.statuses-s2', size);
  await popover(page).getByRole('button', { name: 'Skip tour' }).click();
  await expect.poll(() => lastWrite(writes, 'tasks.statuses')?.status).toBe('skipped');
  expect(errors).toEqual([]);
}, { phone: false });

driveTour('time.checkpoints: add a checkpoint completes the tour', async ({ page, size }) => {
  const { writes, errors } = await boot(page, size, { timers: true });
  await startFromHub(page, 'time.checkpoints');
  await primary(page, 'Take me there').click();
  await expect(page).toHaveURL(/\/demo\/list$/);
  await page.locator('[data-task-id="1"]').first().click();
  await attached(page, ['task-form.checkpoints']);
  await expect(popover(page)).toContainText('checkpoint');
  await shots(page, 'time.checkpoints-s1', size);
  await page.getByRole('button', { name: /Add entry/ }).click();
  await expect.poll(() => lastWrite(writes, 'time.checkpoints')?.status).toBe('completed');
  expect(errors).toEqual([]);
}, { phone: false });

driveTour('time.pomodoro: take me to the task, anchors appear after enabling', async ({ page, size }) => {
  const { writes, errors } = await boot(page, size);
  await page.route('**/api/tasks/1/pomodoro**', (route) => {
    const method = route.request().method();
    if (method === 'GET') return route.fulfill({ status: 404, json: { message: 'none' } });
    return route.fulfill({
      json: {
        data: {
          id: 1, task_id: 1, user_id: 1, phase: 'focus', running: true, phase_started_at: new Date().toISOString(),
          remaining_ms: 1500000, completed_pomodoros: 0, distractions_count: 0, updated_at: new Date().toISOString(),
        },
      },
    });
  });
  await startFromHub(page, 'time.pomodoro');
  await primary(page, 'Take me there').click();
  await page.locator('[data-task-id="1"]').first().click();
  await attached(page, ['task-form.pomodoro']);
  await shots(page, 'time.pomodoro-s1', size);
  await page.locator('[data-tour="task-form.pomodoro"]').first().click();
  await expect.poll(() => lastWrite(writes, 'time.pomodoro')?.step_id ?? lastWrite(writes, 'time.pomodoro')?.status).toMatch(/s2|completed/);
  await attached(page, ['pomodoro.settings']);
  await expectClear(page, 'pomodoro.settings');
  await shots(page, 'time.pomodoro-s2', size);
  expect(errors).toEqual([]);
});

driveTour('time.export: select, open actions, export panel, download', async ({ page, size }) => {
  const exportUrls = [];
  page.on('request', (request) => {
    if (request.method() === 'GET' && /\/api\/exports\/tasks\//.test(request.url())) exportUrls.push(request.url());
  });
  const { writes, errors } = await boot(page, size);
  await startFromHub(page, 'time.export');
  await expect(page).toHaveURL(/\/demo\/list$/);
  await expectClear(page, 'list.select');
  await shots(page, 'fu-ti5-s1', size);
  await page.locator('[data-tour="list.select"]').first().click();
  await page.getByRole('button', { name: 'Select all' }).click();
  await attached(page, ['list.process']);
  await expect.poll(() => lastWrite(writes, 'time.export')?.step_id).toBe('s2');
  await expect(popover(page)).toContainText('Open actions');
  await shots(page, 'fu-ti5-s2', size);
  await page.locator('[data-tour="list.process"]').first().click();
  await expect.poll(() => lastWrite(writes, 'time.export')?.step_id).toBe('s3');
  await attached(page, ['export.panel', 'export.download']);
  await expect(page.locator('[data-tour="export.panel"]').first()).toBeVisible();
  await expect(popover(page)).toContainText('Export settings');
  await shots(page, 'fu-ti5-s3-panel', size);
  await primary(page, 'Next').click();
  await expect.poll(() => lastWrite(writes, 'time.export')?.step_id).toBe('s4');
  await shots(page, 'fu-ti5-s4', size);
  await page.locator('[data-tour="export.download"]').first().click();
  await expect.poll(() => exportUrls.length).toBe(1);
  await expect.poll(() => lastWrite(writes, 'time.export')?.status).toBe('completed');
  await shots(page, 'fu-ti5-done', size);
  expect(errors).toEqual([]);
}, { phone: false });

const openExportModal = async (page) => {
  await page.goto('/demo/list');
  await page.locator('[data-tour="list.select"]').first().click();
  await page.getByRole('button', { name: 'Select all' }).click();
  await page.locator('[data-tour="list.process"]').first().click();
};

e2e('export panel: no AI checkbox, untouched request equals the backend defaults, changes are sent', async ({ page }) => {
  const size = SIZES[0];
  const exportUrls = [];
  page.on('request', (request) => {
    if (request.method() === 'GET' && /\/api\/exports\/tasks\//.test(request.url())) exportUrls.push(request.url());
  });
  const { errors } = await boot(page, size);
  await openExportModal(page);
  const panel = page.locator('[data-tour="export.panel"]').first();
  await expect(panel).toBeVisible();
  await expect(panel).not.toContainText(/AI|explanation/i);
  await expect(page.getByText(/AI explanations/i)).toHaveCount(0);
  await expect(panel.getByLabel('Zero expected hours on off-days')).not.toBeChecked();
  await expect(panel.getByLabel('Parse date from task title')).toBeChecked();
  await expect(panel.getByLabel('Round hours up')).not.toBeChecked();
  await shoot(page, 'fu-ti5-panel-defaults', size, 'light');
  await shoot(page, 'fu-ti5-panel-defaults', size, 'dark');
  await setScheme(page, 'light');
  await page.locator('[data-tour="export.download"]').first().click();
  await expect.poll(() => exportUrls.length).toBe(1);
  const first = new URL(exportUrls[0]);
  expect(first.pathname).toMatch(/\/exports\/tasks\/csv$/);
  for (const key of first.searchParams.keys()) expect(key, key).toMatch(/^(ids\[\d+\]|per_hour)$/);
  for (const key of ['zero_expected_weekends', 'round_hours_up', 'parse_date_from_title', 'weekend_days[0]', 'custom_holidays[0]', 'include_explanations', 'week_start'])
    expect(first.searchParams.has(key), key).toBe(false);

  await openExportModal(page);
  await panel.getByLabel('Zero expected hours on off-days').check();
  await panel.getByLabel('Parse date from task title').uncheck();
  await panel.getByLabel('Round hours up').check();
  await panel.locator('input[type="date"]').fill('2026-10-12');
  await panel.getByRole('button', { name: 'Add' }).click();
  await shoot(page, 'fu-ti5-panel-changed', size, 'light');
  await page.locator('[data-tour="export.download"]').first().click();
  await expect.poll(() => exportUrls.length).toBe(2);
  const second = new URL(exportUrls[1]).searchParams;
  expect(second.get('zero_expected_weekends')).toBe('true');
  expect(second.get('parse_date_from_title')).toBe('false');
  expect(second.get('round_hours_up')).toBe('true');
  expect(second.get('custom_holidays[0]')).toBe('2026-10-12');
  expect(second.has('include_explanations')).toBe(false);
  expect(errors).toEqual([]);
});

e2e('export panel and time.export are absent when the exports module is off', async ({ page }) => {
  const size = SIZES[0];
  const { errors } = await boot(page, size, { modules: ['pages', 'routines'] });
  await openExportModal(page);
  await expect(page.getByText('Batch Actions')).toBeVisible();
  await expect(page.locator('[data-tour="export.panel"]')).toHaveCount(0);
  await shoot(page, 'fu-ti5-module-off', size, 'light');
  await page.keyboard.press('Escape');
  await page.goto('/demo/getting-started');
  await expect(page.locator('[data-tour-id="time.export"]')).toContainText('Turn on Exports');
  expect(errors).toEqual([]);
});

driveTour('pages.first: new page advances to the page tree', async ({ page, size, mobile }) => {
  const { writes, calls, errors } = await boot(page, size);
  await startFromHub(page, 'pages.first');
  await primary(page, 'Take me there').click();
  await expect(page).toHaveURL(/\/demo\/pages$/);
  await attached(page, ['pages.new']);
  await shots(page, 'pages.first-s1', size);
  await page.locator('[data-tour="pages.new"]').first().click();
  await expect.poll(() => calls.includes('POST pages')).toBe(true);
  await expect.poll(() => lastWrite(writes, 'pages.first')?.step_id).toBe('s2');
  await expect(popover(page)).toContainText('Page tree');
  if (!mobile) await expectClear(page, 'pages.tree');
  await shots(page, 'pages.first-s2', size);
  await primary(page, 'Done').click();
  await expect.poll(() => lastWrite(writes, 'pages.first')?.status).toBe('completed');
  expect(errors).toEqual([]);
});

driveTour('pages.versions: no page yet shows the take-me message, opening a page continues', async ({ page, size }) => {
  const { writes, errors } = await boot(page, size);
  await startFromHub(page, 'pages.versions');
  await expect(popover(page)).toContainText('could not find this');
  await primary(page, 'Take me there').click();
  await expect(page).toHaveURL(/\/demo\/pages$/);
  await shots(page, 'pages.versions-take', size);
  await page.goto('/demo/pages/handbook');
  await attached(page, ['page.versions']);
  await page.keyboard.press('Escape');
  expect(writes.length).toBeGreaterThan(0);
  expect(errors).toEqual([]);
}, { phone: false });

driveTour('pages.import: spot import then export tip', async ({ page, size }) => {
  const { writes, errors } = await boot(page, size);
  await startFromHub(page, 'pages.import');
  await expectClear(page, 'pages.import');
  await shots(page, 'pages.import-s1', size);
  await primary(page, 'Next').click();
  await expect.poll(() => lastWrite(writes, 'pages.import')?.step_id).toBe('s2');
  await expectClear(page, 'pages.export');
  await shots(page, 'pages.import-s2', size);
  await primary(page, 'Done').click();
  await expect.poll(() => lastWrite(writes, 'pages.import')?.status).toBe('completed');
  expect(errors).toEqual([]);
}, { phone: false });

driveTour('pages tree and type picker tours point at the sidebar', async ({ page, size }) => {
  const { errors } = await boot(page, size);
  await startFromHub(page, 'pages.people-meetings');
  await expectClear(page, 'pages.type-picker');
  await shots(page, 'pages.people-meetings', size);
  await page.keyboard.press('Escape');
  expect(errors).toEqual([]);
}, { phone: false });

driveTour('graph.map: three steps over the map', async ({ page, size }) => {
  const { writes, errors } = await boot(page, size);
  await startFromHub(page, 'graph.map');
  await expect(page).toHaveURL(/\/demo\/map$/);
  await expectClear(page, 'map.canvas');
  await shots(page, 'graph.map-s1', size);
  await primary(page, 'Next').click();
  await expect.poll(() => lastWrite(writes, 'graph.map')?.step_id).toBe('s2');
  await expectClear(page, 'map.filters');
  await shots(page, 'graph.map-s2', size);
  await primary(page, 'Next').click();
  await expect.poll(() => lastWrite(writes, 'graph.map')?.step_id).toBe('s3');
  await expectClear(page, 'map.search');
  await shots(page, 'graph.map-s3', size);
  await primary(page, 'Done').click();
  await expect.poll(() => lastWrite(writes, 'graph.map')?.status).toBe('completed');
  expect(errors).toEqual([]);
}, { phone: false });

driveTour('graph.enable: switching Graph on completes and unlocks the map tour', async ({ page, size }) => {
  const { writes, modules, errors } = await boot(page, size, { graph: false });
  await page.goto('/demo/getting-started');
  await expect(page.locator('[data-tour-id="graph.map"]')).toContainText('Turn on Graph in Settings -> Modules');
  await page.locator('[data-tour-id="graph.map"]').scrollIntoViewIfNeeded();
  await shots(page, 'graph.hub-off', size);
  await hubStart(page, 'graph.enable').click();
  await expect(page).toHaveURL(/\/settings\/modules$/);
  await expectClear(page, 'modules.graph');
  await shots(page, 'graph.enable-s1', size);
  await page.locator('[data-tour="modules.graph"]').first().click();
  await expect.poll(() => modules.graph).toBe(true);
  await expect.poll(() => lastWrite(writes, 'graph.enable')?.status).toBe('completed');
  await page.goto('/demo/getting-started');
  await expect(hubStart(page, 'graph.map')).toBeVisible();
  expect(errors).toEqual([]);
}, { phone: false });

driveTour('routines.first: quick add advances, tick it off', async ({ page, size, mobile }) => {
  const { writes, calls, errors } = await boot(page, size);
  await startFromHub(page, 'routines.first');
  await primary(page, 'Take me there').click();
  await expect(page).toHaveURL(/\/routines$/);
  await attached(page, [mobile ? 'routines.quick-add' : 'routines.add']);
  await shots(page, 'routines.first-s1', size);
  await page.locator('[data-tour="routines.quick-add"]').first().fill('Water plants');
  await page.keyboard.press('Enter');
  await expect.poll(() => calls.includes('POST routines')).toBe(true);
  await expect.poll(() => lastWrite(writes, 'routines.first')?.step_id).toBe('s2');
  await expect(popover(page)).toContainText('Tick it off');
  await shots(page, 'routines.first-s2', size);
  await page.locator('[data-tour="routines.instance"]').first().click();
  await expect.poll(() => lastWrite(writes, 'routines.first')?.status).toBe('completed');
  expect(errors).toEqual([]);
});

driveTour('routines.views: spot the switcher and done', async ({ page, size }) => {
  const { writes, errors } = await boot(page, size);
  await startFromHub(page, 'routines.views');
  await expect(page).toHaveURL(/\/routines$/);
  await expectClear(page, 'routines.view-switcher');
  await shots(page, 'routines.views', size);
  await primary(page, 'Done').click();
  await expect.poll(() => lastWrite(writes, 'routines.views')?.status).toBe('completed');
  expect(errors).toEqual([]);
});

driveTour('routines.reminders: open a routine, reminder and convert steps', async ({ page, size }) => {
  const { writes, errors } = await boot(page, size);
  await startFromHub(page, 'routines.reminders');
  await primary(page, 'Take me there').click();
  await expect(page).toHaveURL(/\/routines$/);
  await attached(page, ['routines.instance']);
  await shots(page, 'routines.reminders-s1', size);
  await page.getByText('Stretch', { exact: true }).first().click();
  await expect.poll(() => lastWrite(writes, 'routines.reminders')?.step_id).toBe('s2');
  await attached(page, ['routine.reminder']);
  await expect(popover(page).locator('.tour-title, h2, h3, [id^="tour-title"]').first()).toHaveText('Reminder');
  await shots(page, 'routines.reminders-s2', size);
  await primary(page, 'Next').click();
  await expect.poll(() => lastWrite(writes, 'routines.reminders')?.step_id).toBe('s3');
  await attached(page, ['routine.to-task']);
  await shots(page, 'routines.reminders-s3', size);
  await primary(page, 'Done').click();
  await expect.poll(() => lastWrite(writes, 'routines.reminders')?.status).toBe('completed');
  expect(errors).toEqual([]);
}, { phone: false });

driveTour('routines.reminders: no routines shows the take-me message', async ({ page, size }) => {
  const { errors } = await boot(page, size);
  await page.route('**/api/daily-routines/expand**', (route) => route.fulfill({ json: { data: [] } }));
  await startFromHub(page, 'routines.reminders');
  await expect(popover(page)).toContainText('could not find this');
  await shots(page, 'routines.reminders-empty', size);
  await page.keyboard.press('Escape');
  expect(errors).toEqual([]);
}, { phone: false });

driveTour('routines.first: skip this step on the wait step moves on', async ({ page, size, mobile }) => {
  const { writes, errors } = await boot(page, size);
  await startFromHub(page, 'routines.first');
  await skipStep(page, mobile);
  await expect.poll(() => lastWrite(writes, 'routines.first')?.step_id ?? lastWrite(writes, 'routines.first')?.status, { timeout: 15000 }).toMatch(/s2|completed|skipped/);
  expect(errors).toEqual([]);
});

e2e('every S4b anchor is in the DOM on its page, conditional ones after acting', async ({ page }) => {
  await boot(page, SIZES[0], { timers: true });
  await page.goto('/demo/board');
  await attached(page, ['statuses.editor']);
  await page.goto('/demo/categories');
  await attached(page, ['categories.create', 'categories.row']);
  await page.goto('/demo/files');
  await attached(page, ['files.list']);
  await page.goto('/demo/list');
  await attached(page, ['list.select', 'nav.active-timers']);
  await page.locator('[data-tour="list.select"]').first().click();
  await page.getByRole('button', { name: 'Select all' }).click();
  await attached(page, ['list.process']);
  await page.getByRole('button', { name: /^Process/ }).click();
  await attached(page, ['export.panel', 'export.download']);
  await page.keyboard.press('Escape');
  await openFirstTask(page);
  await attached(page, ['task-form.relations', 'task-form.checkpoints', 'task-form.pomodoro', 'task-form.estimate']);
  await page.goto('/demo/pages');
  await attached(page, ['pages.new', 'pages.import', 'pages.export', 'pages.tree', 'pages.type-picker']);
  await page.goto('/demo/pages/handbook');
  await attached(page, ['page.editor', 'page.versions', 'page.backlinks']);
  await page.goto('/demo/map');
  await attached(page, ['map.canvas', 'map.filters', 'map.search']);
  await page.goto('/settings/modules');
  await attached(page, ['modules.graph']);
  await page.goto('/routines');
  await attached(page, ['routines.add', 'routines.quick-add', 'routines.view-switcher', 'routines.instance']);
  await page.getByText('Stretch', { exact: true }).first().click();
  await attached(page, ['routine.reminder', 'routine.to-task']);
});
