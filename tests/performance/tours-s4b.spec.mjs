import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { mockApp } from './mockApp.mjs';
import { SCHEMES, SIZES, mockOnboarding, setScheme } from './toursHelpers.mjs';
import { mockGraph, mockMisc, mockModules, mockPages, mockRoutines } from './toursS4bMocks.mjs';

const e2e = process.env.S4_E2E === '1' ? test : test.skip;

const SHOT_DIR = process.env.TOURS_S4B_SHOTS ?? `${process.env.HOME}/projects/own-projects/tmgr.dev/docs/reports/tours/s4b`;
mkdirSync(SHOT_DIR, { recursive: true });

const shoot = async (page, name, size, scheme) => {
  await setScheme(page, scheme);
  await page.screenshot({ path: `${SHOT_DIR}/${name}-${size.name}-${scheme}.png` });
};

const mockS4b = async (page, size, { graph = true } = {}) => {
  await page.setViewportSize({ width: size.width, height: size.height });
  await mockApp(page, { multiStatus: true });
  const fixture = await mockOnboarding(page, { tips_mode: 'hub_only' });
  const calls = [];
  const modules = await mockModules(page, { graph });
  await mockPages(page, calls);
  await mockRoutines(page, calls);
  await mockGraph(page);
  await mockMisc(page, calls);
  return { ...fixture, calls, modules };
};

const popover = (page) => page.locator('[role="dialog"][aria-labelledby^="tour-"]').first();
const lastWrite = (writes, id) => writes.filter((w) => w.path === `onboarding/tours/${id}`).at(-1)?.body;
const hubStart = (page, id) => page.getByTestId(`hub-start-${id}`);

const expectAnchors = async (page, anchors) => {
  for (const anchor of anchors) {
    await expect(page.locator(`[data-tour="${anchor}"]`).first(), anchor).toBeAttached();
  }
};

const openFirstTask = async (page) => {
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
};

const groups = [
  {
    name: 'tasks',
    tours: [
      { id: 'tasks.statuses', mobile: false },
      { id: 'tasks.categories', mobile: true },
      { id: 'tasks.relations', mobile: false },
      { id: 'tasks.files-page', mobile: true },
      { id: 'tasks.multi-select', mobile: false },
    ],
    driven: 'tasks.categories',
    anchorsByPage: async (page, size) => {
      await page.goto('/demo/board');
      await expectAnchors(page, ['statuses.editor']);
      await page.goto('/demo/categories');
      await expectAnchors(page, ['categories.create', 'categories.row']);
      await page.goto('/demo/files');
      await expectAnchors(page, ['files.list']);
      await page.goto('/demo/list');
      await expectAnchors(page, ['list.select']);
      await openFirstTask(page);
      await expectAnchors(page, ['task-form.relations']);
    },
    drive: async (page, calls) => {
      await page.goto('/demo/categories/create');
      await page.getByPlaceholder('Name').fill('Fixture');
      await page.getByPlaceholder('e.g., PROJ, FEAT, BUG').fill('FIX');
      await page.getByRole('button', { name: 'Create', exact: true }).click();
      await expect.poll(() => calls.includes('POST project_categories')).toBe(true);
    },
  },
  {
    name: 'time',
    tours: [
      { id: 'time.parallel', mobile: false },
      { id: 'time.checkpoints', mobile: false },
      { id: 'time.pomodoro', mobile: true },
      { id: 'time.export', mobile: false },
    ],
    driven: 'time.checkpoints',
    anchorsByPage: async (page, size) => {
      await page.goto('/demo/list');
      if (size.width >= 640) {
        await expectAnchors(page, ['nav.active-timers']);
        await page.locator('[data-tour="list.select"]').first().click();
        await expectAnchors(page, ['list.process']);
      }
      await openFirstTask(page);
      await expectAnchors(page, ['task-form.checkpoints', 'task-form.pomodoro', 'task-form.estimate']);
    },
    drive: async (page) => {
      await openFirstTask(page);
      await page.getByRole('button', { name: /Add entry/ }).click();
    },
  },
  {
    name: 'pages',
    tours: [
      { id: 'pages.first', mobile: true },
      { id: 'pages.context', mobile: false },
      { id: 'pages.links', mobile: false },
      { id: 'pages.versions', mobile: false },
      { id: 'pages.people-meetings', mobile: false },
      { id: 'pages.import', mobile: false },
      { id: 'pages.task-from-selection', mobile: false },
    ],
    driven: 'pages.first',
    anchorsByPage: async (page, size) => {
      await page.goto('/demo/pages');
      await expectAnchors(page, ['pages.new', 'pages.import', 'pages.export']);
      if (size.width >= 640) await expectAnchors(page, ['pages.tree', 'pages.type-picker']);
      await page.goto('/demo/pages/handbook');
      await expectAnchors(page, ['page.editor', 'page.versions', 'page.backlinks']);
    },
    drive: async (page, calls) => {
      await page.goto('/demo/pages');
      await page.locator('[data-tour="pages.new"]').first().click();
      await expect.poll(() => calls.includes('POST pages')).toBe(true);
    },
  },
  {
    name: 'graph',
    graphOff: true,
    tours: [
      { id: 'graph.map', mobile: false },
      { id: 'graph.enable', mobile: false },
    ],
    driven: 'graph.enable',
    anchorsByPage: async (page) => {
      await page.goto('/demo/map');
      await expectAnchors(page, ['map.canvas', 'map.filters', 'map.search']);
      await page.goto('/settings/modules');
      await expectAnchors(page, ['modules.graph']);
    },
    drive: async (page) => {
      await page.goto('/settings/modules');
      await page.locator('[data-tour="modules.graph"]').first().click();
    },
  },
  {
    name: 'routines',
    tours: [
      { id: 'routines.first', mobile: true },
      { id: 'routines.views', mobile: true },
      { id: 'routines.reminders', mobile: false },
    ],
    driven: 'routines.first',
    anchorsByPage: async (page) => {
      await page.goto('/routines');
      await expectAnchors(page, ['routines.add', 'routines.quick-add', 'routines.view-switcher', 'routines.instance']);
      await page.getByText('Stretch', { exact: true }).first().click();
      await expectAnchors(page, ['routine.reminder', 'routine.to-task']);
    },
    drive: async (page, calls) => {
      await page.goto('/routines');
      await page.locator('[data-tour="routines.quick-add"]').first().fill('Water plants');
      await page.keyboard.press('Enter');
      await expect.poll(() => calls.includes('POST routines')).toBe(true);
    },
  },
];

for (const group of groups) {
  for (const size of SIZES) {
    for (const scheme of SCHEMES) {
      e2e(`${group.name}: hub, anchors and one wait step at ${size.name}, ${scheme}`, async ({ page }) => {
        const mobile = size.width < 640;
        await page.emulateMedia({ colorScheme: scheme });
        const { writes, calls, modules } = await mockS4b(page, size, { graph: !(group.graphOff && !mobile) });

        await page.goto('/demo/getting-started');
        await setScheme(page, scheme);

        for (const tour of group.tours) {
          if (mobile && !tour.mobile) {
            await expect(hubStart(page, tour.id), `unavailable: platform ${tour.id}`).toHaveCount(0);
          }
        }

        if (group.graphOff && !mobile) {
          await expect(page.getByText(/Turn on Graph in Settings -> Modules/).first()).toBeVisible();
          await expect(hubStart(page, 'graph.map')).toHaveCount(0);
        } else {
          const first = group.tours.find((tour) => !mobile || tour.mobile);
          if (first) {
            await hubStart(page, first.id).click();
            await expect(popover(page)).toBeVisible();
            await shoot(page, `${group.name}-${first.id}`, size, scheme);
            await page.keyboard.press('Escape');
            await expect(popover(page)).toBeHidden();
          }
        }

        await group.anchorsByPage(page, size);

        const driven = group.tours.find((tour) => tour.id === group.driven);
        if (!mobile || driven.mobile) {
          await page.goto('/demo/getting-started');
          await hubStart(page, group.driven).click();
          await expect(popover(page)).toBeVisible();
          await group.drive(page, calls);
          await expect
            .poll(() => lastWrite(writes, group.driven)?.status)
            .toMatch(/in_progress|completed/);
          await expect
            .poll(() => {
              const body = lastWrite(writes, group.driven);
              return body?.status === 'completed' || body?.step_id !== 's1';
            })
            .toBe(true);
          await shoot(page, `${group.name}-${group.driven}-advanced`, size, scheme);
          if (group.graphOff && !mobile) {
            await expect.poll(() => modules.graph).toBe(true);
            await page.goto('/demo/getting-started');
            await expect(hubStart(page, 'graph.map')).toBeVisible();
          }
        }
      });
    }
  }
}
