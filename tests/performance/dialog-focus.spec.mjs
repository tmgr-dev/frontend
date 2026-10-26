import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';
import { mockModules } from './toursS4bMocks.mjs';

// TM-733: Modal.vue, TaskSidePanel.vue and the Pomodoro settings dialog are real dialogs: role,
// label, focus moved in, Tab trapped (only while on top of the modal stack), focus returned to the
// opener; closeOnBgClick closes, its absence keeps the wobble; Esc in Pomodoro settings closes only
// that dialog; a failed save keeps the draft and shows a retryable error.

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
});

const insideActive = (page, selector) =>
  page.evaluate(
    (s) => !!document.querySelector(s)?.contains(document.activeElement),
    selector,
  );

const tabAround = async (page, selector, presses, shift = false) => {
  for (let i = 0; i < presses; i++) {
    await page.keyboard.press(shift ? 'Shift+Tab' : 'Tab');
    expect(
      await insideActive(page, selector),
      `focus left ${selector} after press ${i + 1}`,
    ).toBe(true);
  }
};

// The pull-back masks a broken wrap (focus leaves, comes back to the same element), so the wrap is
// asserted directly: Tab on the last focusable lands on the first, Shift+Tab on the first on the last.
const expectWrap = async (page, selector) => {
  const count = await page.evaluate((s) => {
    const items = Array.from(
      document
        .querySelector(s)
        .querySelectorAll(
          'a[href],button:not([disabled]),input:not([disabled]):not([type="hidden"]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])',
        ),
    ).filter(
      (el) =>
        el.getClientRects().length > 0 &&
        getComputedStyle(el).visibility !== 'hidden' &&
        !el.closest('[inert]'),
    );
    window.__first = items[0];
    window.__last = items[items.length - 1];
    window.__last?.focus();
    return items.length;
  }, selector);
  expect(count).toBeGreaterThan(1);
  await page.keyboard.press('Tab');
  expect(
    await page.evaluate(() => document.activeElement === window.__first),
    `Tab on the last focusable of ${selector} wraps to the first`,
  ).toBe(true);
  await page.keyboard.press('Shift+Tab');
  expect(
    await page.evaluate(() => document.activeElement === window.__last),
    `Shift+Tab on the first focusable of ${selector} wraps to the last`,
  ).toBe(true);
};

const backdrop = (page) =>
  page.locator('[data-name="overlay"]').last().locator(':scope > div').first();

const pomodoroState = {
  id: 1,
  task_id: 1,
  user_id: 1,
  phase: 'focus',
  running: false,
  phase_started_at: null,
  remaining_ms: 1500000,
  completed_pomodoros: 0,
  distractions_count: 0,
  updated_at: '2026-10-26T10:00:00Z',
};

const openPanelWithPomodoro = async (page, { patchStatuses = [200] } = {}) => {
  await mockApp(page);
  await mockModules(page);
  const patches = [];
  await page.route('**/api/tasks/1/pomodoro**', (route) =>
    route.fulfill({ json: { data: pomodoroState } }),
  );
  await page.route('**/api/user/settings', (route) => {
    if (route.request().method() !== 'PATCH') return route.fallback();
    patches.push(route.request().postDataJSON());
    const status =
      patchStatuses[Math.min(patches.length, patchStatuses.length) - 1];
    return status === 200
      ? route.fulfill({ json: { data: [] } })
      : route.fulfill({ status, json: { message: 'boom' } });
  });
  await page.goto('/demo/list');
  const row = page.locator('[data-task-id="1"]').first();
  await row.click({ timeout: 60000 });
  const panel = page.locator('[data-name="task-side-panel"]');
  await expect(panel).toHaveCount(1);
  await page.getByTitle('Pomodoro settings').waitFor();
  return { panel, row, patches };
};

test.describe('dialogs', () => {
  test.use({ viewport: { width: 1280, height: 800 } });
  test.setTimeout(90000);

  test('Modal: labelled modal dialog, focus in, Tab trapped, backdrop cancels a Confirm, focus returns', async ({
    page,
  }) => {
    await mockApp(page, { taskCount: 3 });
    await page.goto('/demo/board');
    const card = page.locator('[data-task-id="2"]');
    await card.waitFor();
    await card.hover();
    await card.getByRole('button', { name: 'Task actions' }).click();
    await page.getByRole('menuitem', { name: 'Delete' }).click();

    const dialog = page.locator('[data-name="overlay"] [role="dialog"]');
    await expect(dialog).toHaveCount(1);
    await expect(dialog).toHaveAttribute('aria-modal', 'true');
    const name = await dialog.evaluate(
      (el) =>
        el.getAttribute('aria-label') || el.getAttribute('aria-labelledby'),
    );
    expect(name, 'the dialog is labelled').toBeTruthy();
    await expect
      .poll(() => insideActive(page, '[data-name="overlay"] [role="dialog"]'))
      .toBe(true);

    await tabAround(page, '[data-name="overlay"] [role="dialog"]', 6);
    await tabAround(page, '[data-name="overlay"] [role="dialog"]', 6, true);
    await expectWrap(page, '[data-name="overlay"] [role="dialog"]');

    await backdrop(page).click({ position: { x: 5, y: 5 } });
    await expect(dialog).toHaveCount(0);
    await expect(card).toBeVisible();
    await expect(
      card.getByRole('button', { name: 'Task actions' }),
      'focus is back on the menu trigger that led to the dialog',
    ).toBeFocused();
  });

  test('Modal with a heading is labelled by it; closeOnBgClick closes, its absence only wobbles', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 800, height: 900 });
    await mockApp(page, { multiStatus: true });
    await page.goto('/demo/board');
    const filters = page.getByTitle('Filters');
    await filters.click();

    const dialog = page.locator('[data-name="overlay"] [role="dialog"]');
    await expect(dialog).toHaveCount(1);
    await backdrop(page).click({ position: { x: 5, y: 5 } });
    await expect(dialog, 'filters modal has closeOnBgClick').toHaveCount(0);
    await expect(filters).toBeFocused();

    await filters.click();
    await dialog.getByText('Add status').first().click();
    const status = page.getByRole('dialog', { name: 'Create status' });
    await expect(status).toBeVisible();
    await expect(status).toHaveAttribute('aria-modal', 'true');
    await backdrop(page).click({ position: { x: 5, y: 5 } });
    await expect(
      status
        .locator('xpath=..')
        .locator('.zoom-effect, [class*="zoom-effect"]'),
    ).toHaveCount(1);
    await expect(status, 'status form has no closeOnBgClick').toBeVisible();

    await page.keyboard.press('Escape');
    await expect(status).toHaveCount(0);
  });

  test('task panel: labelled dialog, focus in, Tab trapped, Esc returns focus to the opener', async ({
    page,
  }) => {
    await mockApp(page);
    await page.goto('/demo/list');
    const row = page.locator('[data-task-id="1"]').first();
    await row.click({ timeout: 60000 });
    const aside = page.locator('[data-name="task-side-panel"] aside');
    await expect(aside).toHaveAttribute('role', 'dialog');
    await expect(aside).toHaveAttribute('aria-modal', 'true');
    await expect(aside).toHaveAccessibleName('Task');
    await expect(
      aside.locator('textarea[data-tour="task-form.title"]'),
    ).toBeVisible();
    await expect
      .poll(() => insideActive(page, '[data-name="task-side-panel"] aside'))
      .toBe(true);

    await tabAround(page, '[data-name="task-side-panel"] aside', 40);
    await tabAround(page, '[data-name="task-side-panel"] aside', 40, true);
    await expectWrap(page, '[data-name="task-side-panel"] aside');

    await page.keyboard.press('Escape');
    await expect(page.locator('[data-name="task-side-panel"]')).toHaveCount(0);
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            document.activeElement?.closest('[data-task-id]')?.dataset.taskId,
        ),
      )
      .toBe('1');
  });

  test('task panel: a menu teleported out of it keeps the focus, no pull-back', async ({
    page,
  }) => {
    await mockApp(page);
    await page.goto('/demo/list');
    await page.locator('[data-task-id="1"]').first().click({ timeout: 60000 });
    await page.getByTestId('task-share-trigger').click();
    const menu = page.getByRole('menu');
    await expect(menu).toHaveAttribute('data-state', 'open');
    await page.keyboard.press('ArrowDown');
    await page.waitForTimeout(400);
    expect(
      await page.evaluate(
        () => !!document.activeElement?.closest('[role="menu"]'),
      ),
      'focus is inside the open menu',
    ).toBe(true);
    await page.keyboard.press('Escape');
    await expect(menu).toHaveCount(0);
    await expect(page.getByTestId('task-share-trigger')).toBeFocused();
  });

  test('Pomodoro settings: nested over the panel, own trap, Esc closes only it, focus returns', async ({
    page,
  }) => {
    const { panel } = await openPanelWithPomodoro(page);
    const opener = page.getByTitle('Pomodoro settings');
    await opener.click();
    const dialog = page.getByRole('dialog', { name: 'Pomodoro' });
    await expect(dialog).toBeVisible();
    await expect(dialog).toHaveAttribute('aria-modal', 'true');
    const own = '[role="dialog"][aria-labelledby^="pomodoro-settings-"]';
    await expect.poll(() => insideActive(page, own)).toBe(true);
    await tabAround(page, own, 40);
    await tabAround(page, own, 40, true);
    await expectWrap(page, own);

    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(panel).toHaveCount(1);
    await expect(opener).toBeFocused();

    await page.keyboard.press('Tab');
    expect(
      await insideActive(page, '[data-name="task-side-panel"] aside'),
      'the panel trap is active again once the settings dialog is gone',
    ).toBe(true);

    await page.waitForTimeout(1100);
    await page.keyboard.press('Escape');
    await expect(panel).toHaveCount(0);
  });

  const openConfirmOverPanel = async (page) => {
    await mockApp(page, { multiStatus: true });
    await mockModules(page);
    await page.goto('/demo/list');
    const started = await page.evaluate(() =>
      fetch('/api/tasks/1').then((r) => r.json()),
    );
    await page.route('**/api/tasks/1/countdown**', (route) =>
      route.fulfill({
        json: { data: { ...started.data, start_time: 1, status_id: 1 } },
      }),
    );
    await page.locator('[data-task-id="1"]').first().click({ timeout: 60000 });
    const panel = page.locator('[data-name="task-side-panel"]');
    await expect(panel).toHaveCount(1);
    const start = page.getByTitle(/^(Start|Stop) timer$/);
    await start.click();
    const own = '[data-name="overlay"] [role="dialog"]';
    const dialog = page.locator(own);
    await expect(dialog).toHaveCount(1);
    return { panel, start, own, dialog };
  };

  test('Confirm over the task panel: own trap, the panel trap stands down, focus returns into the panel', async ({
    page,
  }) => {
    const { panel, start, own, dialog } = await openConfirmOverPanel(page);
    await expect(dialog).toHaveAttribute('aria-modal', 'true');
    await expect.poll(() => insideActive(page, own)).toBe(true);
    await tabAround(page, own, 10);
    await tabAround(page, own, 10, true);
    await expectWrap(page, own);

    await dialog.getByRole('button', { name: 'Cancel' }).click();
    await expect(dialog).toHaveCount(0);
    await expect(panel).toHaveCount(1);
    await expect(
      start,
      'focus returned to the opener inside the panel',
    ).toBeFocused();
    await tabAround(page, '[data-name="task-side-panel"] aside', 5);
  });

  test('Confirm over the task panel: backdrop click cancels it and the panel stays', async ({
    page,
  }) => {
    const { panel, start, dialog } = await openConfirmOverPanel(page);
    await backdrop(page).click({ position: { x: 5, y: 5 } });
    await expect(dialog).toHaveCount(0);
    await expect(panel).toHaveCount(1);
    await expect(start).toBeFocused();
  });

  test('Confirm over the task panel: Esc cancels it and the panel stays', async ({
    page,
  }) => {
    const { panel, start, dialog } = await openConfirmOverPanel(page);
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(panel).toHaveCount(1);
    await expect(start).toBeFocused();
  });

  test('Pomodoro settings: Esc while editing a time field only cancels the edit', async ({
    page,
  }) => {
    const { panel } = await openPanelWithPomodoro(page);
    await page.getByTitle('Pomodoro settings').click();
    const dialog = page.getByRole('dialog', { name: 'Pomodoro' });
    await dialog.getByText('min').first().click();
    const input = dialog.locator('input[data-esc-blur]');
    await expect(input).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(input).toHaveCount(0);
    await expect(dialog).toBeVisible();
    await expect(panel).toHaveCount(1);
  });

  test('Pomodoro settings: a failed save keeps the dialog and draft, shows an error, retry works', async ({
    page,
  }) => {
    const { panel, patches } = await openPanelWithPomodoro(page, {
      patchStatuses: [500, 200],
    });
    await page.getByTitle('Pomodoro settings').click();
    const dialog = page.getByRole('dialog', { name: 'Pomodoro' });
    await dialog.getByRole('button', { name: 'Chime' }).click();
    await dialog.getByRole('button', { name: 'Save' }).click();

    const alert = dialog.getByRole('alert');
    await expect(alert).toHaveText("Couldn't save settings. Try again.");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole('button', { name: 'Save' })).toBeEnabled();
    expect(patches).toHaveLength(1);

    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect(dialog).toHaveCount(0);
    expect(patches).toHaveLength(2);
    expect(patches[1].settings.pomodoro.sound).toBe('chime');
    await expect(panel).toHaveCount(1);
  });

  test('Pomodoro settings follow the theme: light surface, readable chip and toggle', async ({
    page,
  }) => {
    const { panel } = await openPanelWithPomodoro(page);
    await page.getByTitle('Pomodoro settings').click();
    const dialog = page.getByRole('dialog', { name: 'Pomodoro' });
    const bg = await dialog.evaluate(
      (el) => getComputedStyle(el).backgroundColor,
    );
    const rgb = bg
      .match(/[\d.]+/g)
      .slice(0, 3)
      .map(Number);
    const scale = Math.max(...rgb) > 1 ? 255 : 1;
    expect(
      rgb.reduce((a, b) => a + b, 0) / scale,
      `dialog background ${bg} is light in the light theme`,
    ).toBeGreaterThan(2.4);
    await page.keyboard.press('Escape');
    await expect(panel).toHaveCount(1);
  });
});
