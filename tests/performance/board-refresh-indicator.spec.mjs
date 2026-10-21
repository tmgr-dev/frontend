import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const shotsDir = process.env.BOARD_REFRESH_SHOTS_DIR;

test.describe.configure({ timeout: 90_000 });

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
});

const openBoard = async (page, { taskCount, timer = false, dark = false }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  if (dark)
    await page.addInitScript(() => localStorage.setItem('colorScheme', 'dark'));
  const api = await mockApp(page, { taskCount, multiStatus: true });
  if (timer)
    await page.route('**/api/tasks/runned', (route) =>
      route.fulfill({
        json: {
          data: [
            {
              id: 1,
              title: 'Original task',
              workspace_id: 1,
              status_id: 1,
              common_time: 60,
              start_time: Math.floor(Date.now() / 1000),
              approximately_time: 0,
            },
          ],
        },
      }),
    );
  const held = [];
  let gate = null;
  await page.route('**/api/tasks/status/**', async (route) => {
    if (gate) {
      held.push(route.request().url());
      await gate;
    }
    await route.fallback();
  });
  await page.goto('/demo/board');
  const column = page.locator('.board-container__item').first();
  await expect(column).toBeVisible();
  if (taskCount)
    await expect(
      page.locator('.board-card-draggable [data-task-id="1"]').first(),
    ).toBeVisible();
  await expect(page.getByText('Refreshing tasks…')).toHaveCount(0);
  if (timer)
    await expect(page.locator('[data-tour="nav.active-timers"]')).toBeVisible();
  await expect
    .poll(async () => {
      const y = await columnY(column);
      await page.waitForTimeout(300);
      return (await columnY(column)) - y;
    })
    .toBe(0);
  let release;
  return {
    api,
    column,
    held: () => held.length,
    hold: () => {
      gate = new Promise((resolve) => {
        release = resolve;
      });
    },
    release: () => {
      gate = null;
      release();
    },
  };
};

const changeCategory = (page) =>
  page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'updateSelectedCategory',
        5,
      ),
  );

const columnY = async (column) => (await column.boundingBox()).y;

const shot = async (page, name) => {
  if (shotsDir) await page.screenshot({ path: `${shotsDir}/${name}.png` });
};

for (const [label, taskCount] of [
  ['populated', 3],
  ['empty', 0],
]) {
  test(`${label} board keeps columns in place while a filter refreshes tasks`, async ({
    page,
  }) => {
    const board = await openBoard(page, { taskCount });
    const before = await columnY(board.column);
    const requestsBefore = board.api.boardRequests();

    board.hold();
    await changeCategory(page);
    await expect.poll(board.held).toBe(2);
    await expect(
      page.getByRole('status').filter({ hasText: 'Refreshing tasks…' }),
    ).toHaveCount(1);
    const during = await columnY(board.column);
    await shot(page, `${label}-light-refreshing`);

    board.release();
    await expect.poll(board.api.boardRequests).toBe(requestsBefore + 2);
    await expect(page.getByText('Refreshing tasks…')).toHaveCount(0);
    const after = await columnY(board.column);

    expect({ during, after }).toEqual({ during: before, after: before });
  });
}

for (const scheme of ['light', 'dark']) {
  test(`corner spinner shows during refresh beside the active timers ring (${scheme})`, async ({
    page,
  }) => {
    const board = await openBoard(page, {
      taskCount: 3,
      timer: true,
      dark: scheme === 'dark',
    });
    if (scheme === 'dark')
      await expect(page.locator('html')).toHaveClass(/dark/);

    board.hold();
    await changeCategory(page);
    await expect.poll(board.held).toBe(2);
    const region = page
      .getByRole('status')
      .filter({ hasText: 'Refreshing tasks…' });
    await expect(region).toHaveCount(1);
    await shot(page, `timer-${scheme}-refreshing`);

    await expect(region).toHaveAttribute('aria-live', 'polite');
    const label = region.getByText('Refreshing tasks…');
    expect((await label.boundingBox())?.width ?? 0).toBeLessThanOrEqual(1);
    const spinner = region.locator('svg.animate-spin');
    await expect(spinner).toBeVisible();
    const viewport = page.viewportSize();
    const box = await spinner.boundingBox();
    expect(box.x).toBeGreaterThan(viewport.width / 2);
    expect(box.y).toBeGreaterThan(viewport.height / 2);
    const ring = await page
      .locator('[data-tour="nav.active-timers"]')
      .boundingBox();
    const overlaps =
      box.x < ring.x + ring.width &&
      ring.x < box.x + box.width &&
      box.y < ring.y + ring.height &&
      ring.y < box.y + box.height;
    expect(overlaps).toBe(false);
    const handle = await region.elementHandle();

    board.release();
    await expect(page.getByText('Refreshing tasks…')).toHaveCount(0);
    expect(
      await handle.evaluate((el) => ({
        connected: el.isConnected,
        text: el.textContent.trim(),
        spinner: !!el.querySelector('svg.animate-spin'),
      })),
    ).toEqual({ connected: true, text: '', spinner: false });
    await shot(page, `timer-${scheme}-idle`);
  });
}
