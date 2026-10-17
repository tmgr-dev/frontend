import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';
import {
  AUTO,
  SCHEMES,
  SIZES,
  WIDE,
  mockOnboarding,
  mockTasks,
  setScheme,
  shoot,
} from './toursHelpers.mjs';

const ENGINE = process.env.TOURS_E2E_ENGINE === '1';
const engine = ENGINE ? test : test.skip;

const dialogs = (page) =>
  page.locator('[role="dialog"][aria-labelledby^="tour-"]');

const boot = async (
  page,
  size,
  { onboarding = { tips_mode: 'hub_only' }, ...options } = {},
) => {
  await page.setViewportSize({ width: size.width, height: size.height });
  await mockApp(page, { multiStatus: true, ...options });
  const created = await mockTasks(page);
  const fixture = await mockOnboarding(page, onboarding);
  return { ...fixture, created };
};

const popover = (page) =>
  page.locator('[role="dialog"][aria-labelledby^="tour-"]').first();

test.describe('anchors and hub (need the tour engine from W1 for useTours)', () => {
  test('every anchor of the S2 catalog is in the DOM on its page', async ({
    page,
  }) => {
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
      await expect(
        page.locator(`[data-tour="${anchor}"]`).first(),
        anchor,
      ).toBeAttached();
    }
    await page.locator('[data-task-id="1"]').first().click();
    for (const anchor of [
      'task-form.title',
      'task-form.status',
      'task-form.timer',
      'task-form.description',
      'task-form.comments',
    ]) {
      await expect(
        page.locator(`[data-tour="${anchor}"]`).first(),
        anchor,
      ).toBeAttached();
    }
  });

  test('board anchors, with a 44px touch area on the drag handle', async ({
    page,
  }) => {
    await boot(page, SIZES[0]);
    await page.goto('/demo/board');
    await expect(
      page.locator('[data-tour="board.column"]').first(),
    ).toBeAttached();
    const add = page.locator('[data-tour="board.add-in-column"]').first();
    await expect(add).toBeAttached();
    await add.focus();
    await expect(add).toHaveCSS('opacity', '1');
    const handle = page.locator('[data-tour="board.card-handle"]').first();
    await expect(handle).toHaveAttribute('aria-label', 'Drag to move');
    const area = await handle.evaluate((el) => {
      const style = getComputedStyle(el, '::after');
      return {
        width: parseFloat(style.width),
        height: parseFloat(style.height),
      };
    });
    expect(area.width).toBeGreaterThanOrEqual(28);
    expect(area.height).toBeGreaterThanOrEqual(44);
  });

  for (const size of SIZES) {
    test(`the hub lists tours and keeps its controls working at ${size.name}`, async ({
      page,
    }) => {
      const { writes } = await boot(page, size, {
        onboarding: {
          tips_mode: 'hub_only',
          tours: {
            'start.nav': {
              status: 'completed',
              step_id: null,
              version: 1,
              generation: 0,
            },
          },
        },
      });
      await page.goto('/demo/getting-started');
      await expect(
        page.getByRole('heading', { name: 'Getting started', level: 1 }),
      ).toBeVisible();
      for (const anchor of [
        'hub.catalog',
        'hub.tips-toggle',
        'hub.checklist',
      ]) {
        await expect(page.locator(`[data-tour="${anchor}"]`)).toBeVisible();
      }
      await expect(page.getByText('Find your way around')).toBeVisible();
      await expect(
        page.getByText('Done', { exact: true }).first(),
      ).toBeVisible();
      await expect(
        page.getByTestId('hub-start-start.first-task'),
      ).toBeVisible();
      for (const scheme of SCHEMES) await shoot(page, 'hub', size, scheme);

      await page
        .getByRole('switch', { name: 'Show tips automatically' })
        .click();
      await expect
        .poll(
          () => writes.find((w) => w.path === 'onboarding/preferences')?.body,
        )
        .toEqual({ tips_mode: 'auto' });

      await page.getByTestId('hub-hide').click();
      await expect
        .poll(
          () =>
            writes.filter((w) => w.path === 'onboarding/preferences').at(-1)
              ?.body,
        )
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
    await expect(
      page.getByRole('button', { name: 'Reset all tours' }),
    ).toBeVisible();
  });

  test('tips settings change the mode and reset all tours', async ({
    page,
  }) => {
    const { writes } = await boot(page, SIZES[0]);
    await page.goto('/settings?tab=about');
    await page.getByLabel('Never show tips').check();
    await expect
      .poll(() => writes.find((w) => w.path === 'onboarding/preferences')?.body)
      .toEqual({ tips_mode: 'off' });
    await page.getByRole('button', { name: 'Reset all tours' }).click();
    await page.getByTestId('tips-reset-confirm').click();
    await expect
      .poll(() =>
        writes.some(
          (w) => w.method === 'DELETE' && w.path === 'onboarding/tours',
        ),
      )
      .toBe(true);
  });
});

const insideDialog = (page) =>
  page.evaluate(() => !!document.activeElement?.closest('[role="dialog"]'));

const assertTouchTargets = async (locator) => {
  for (const button of await locator.getByRole('button').all()) {
    const box = await button.boundingBox();
    expect(box.height, await button.innerText()).toBeGreaterThanOrEqual(44);
  }
};

const createFromList = async (page, size, created, title) => {
  await page.locator('[data-tour="nav.create-task"]').first().click();
  await expect(popover(page)).toContainText(
    size.width < 640 ? 'tap Create' : 'Ctrl+S',
  );
  await page.locator('[data-tour="task-form.title"]').fill(title);
  await page.keyboard.press('Control+s');
  await expect.poll(() => created.length).toBe(1);
};

test.describe('G2 and TI1 with the real engine (TOURS_E2E_ENGINE=1)', () => {
  for (const size of SIZES) {
    for (const scheme of SCHEMES) {
      engine(
        `G2 starts by itself on the empty list at ${size.name}, ${scheme}`,
        async ({ page }) => {
          const { writes, created } = await boot(page, size, {
            onboarding: AUTO,
          });
          await page.emulateMedia({ colorScheme: scheme });
          await page.goto('/demo/list');
          await setScheme(page, scheme);
          await expect(popover(page)).toContainText(
            'Click here to create a task'.replace(
              'Click',
              size.width < 640 ? 'Tap' : 'Click',
            ),
          );
          await shoot(page, 'g2-step1', size, scheme);
          if (size.width < 640) {
            const box = await popover(page).boundingBox();
            expect(box.y + box.height).toBeGreaterThan(size.height - 40);
            await assertTouchTargets(popover(page));
          }

          await page.locator('[data-tour="nav.create-task"]').first().click();
          await expect(popover(page)).toContainText(
            size.width < 640 ? 'tap Create' : 'Ctrl+S',
          );
          await shoot(page, 'g2-step2', size, scheme);

          await page
            .locator('[data-tour="task-form.title"]')
            .fill('First task from the tour');
          await page.keyboard.press('Control+s');
          await expect.poll(() => created.length).toBe(1);
          await expect
            .poll(
              () =>
                writes
                  .filter((w) => w.path === 'onboarding/tours/start.first-task')
                  .at(-1)?.body,
            )
            .toMatchObject({ status: 'completed', step_id: null });
          await expect(dialogs(page)).toHaveCount(0, { timeout: 8000 });
        },
      );

      engine(
        `TI1 from the hub: take me there, then run a timer at ${size.name}, ${scheme}`,
        async ({ page }) => {
          const { writes } = await boot(page, size);
          await page.route('**/api/tasks/1/countdown', (route) =>
            route.fulfill({ json: { data: { id: 1, start_time: 1 } } }),
          );
          await page.goto('/demo/getting-started');
          await page.getByTestId('hub-start-time.timer').click();
          const take = popover(page).getByRole('button', {
            name: 'Take me there',
          });
          await expect(take).toBeVisible();
          await shoot(page, 'ti1-take-me', size, scheme);
          await take.click();
          await expect(page).toHaveURL(/\/demo\/list$/);
          await page.locator('[data-task-id="1"]').first().click();
          await expect(popover(page)).toContainText('Press play');
          await shoot(page, 'ti1-step1', size, scheme);
          await page
            .locator('[data-tour="task-form.timer"] button')
            .first()
            .click();
          await expect
            .poll(
              () =>
                writes
                  .filter((w) => w.path === 'onboarding/tours/time.timer')
                  .at(-1)?.body,
            )
            .toMatchObject({
              status: 'in_progress',
              step_id: expect.not.stringMatching(/^s1$/),
            });
          await shoot(page, 'ti1-step2', size, scheme);
        },
      );
    }
  }

  for (const scheme of SCHEMES) {
    engine(
      `wide screenshots of the hub and G2 at 1440, ${scheme}`,
      async ({ page }) => {
        const { created } = await boot(page, WIDE, { onboarding: AUTO });
        await page.emulateMedia({ colorScheme: scheme });
        await page.goto('/demo/getting-started');
        await setScheme(page, scheme);
        await expect(popover(page)).toContainText('Your Getting started page');
        await shoot(page, 'hub-tour', WIDE, scheme);
        await page.keyboard.press('Escape');
        await expect(popover(page)).toBeHidden();
        await shoot(page, 'hub', WIDE, scheme);
        await page.goto('/demo/list');
        await page.evaluate(() => sessionStorage.clear());
        await page.reload();
        await expect(popover(page)).toContainText(
          'Click here to create a task',
        );
        await shoot(page, 'g2-step1', WIDE, scheme);
        await page.locator('[data-tour="nav.create-task"]').first().click();
        await expect(popover(page)).toContainText('Ctrl+S');
        await shoot(page, 'g2-step2', WIDE, scheme);
        await page
          .locator('[data-tour="task-form.title"]')
          .fill('Wide first task');
        await page.keyboard.press('Control+s');
        await expect.poll(() => created.length).toBe(1);
        await expect(dialogs(page)).toHaveCount(0, { timeout: 8000 });
      },
    );
  }

  engine(
    'a missing anchor falls back to a floating card that never blocks the page, then picks the step up',
    async ({ page }) => {
      const { writes } = await boot(page, SIZES[0]);
      await page.route('**/api/tasks/1/countdown', (route) =>
        route.fulfill({ json: { data: { id: 1, start_time: 1 } } }),
      );
      await page.goto('/demo/getting-started');
      await page.getByTestId('hub-start-time.timer').click();
      await expect(popover(page)).toContainText('could not find this');
      await expect(
        popover(page).getByRole('button', { name: 'Take me there' }),
      ).toBeVisible();
      await expect(page.locator('.tour-backdrop')).toHaveCount(0);
      await shoot(page, 'fallback', SIZES[0], 'light');
      await popover(page)
        .getByRole('button', { name: 'Take me there' })
        .click();
      await expect(page).toHaveURL(/\/demo\/list$/);
      await page.waitForTimeout(500);
      await page.locator('[data-task-id="1"]').first().click();
      await expect(popover(page)).toContainText('Press play');
      await expect(popover(page)).not.toContainText('could not find this');
      await page
        .locator('[data-tour="task-form.timer"] button')
        .first()
        .click();
      await expect
        .poll(
          () =>
            writes
              .filter((w) => w.path === 'onboarding/tours/time.timer')
              .at(-1)?.body,
        )
        .toMatchObject({ status: 'in_progress' });
    },
  );

  engine(
    'Esc inside the task panel closes it first, then pauses the tour, and the hub offers Resume',
    async ({ page }) => {
      const { writes } = await boot(page, SIZES[0]);
      await page.goto('/demo/getting-started');
      await page.getByTestId('hub-start-start.first-task').click();
      await page.locator('[data-tour="nav.create-task"]').first().click();
      await expect(popover(page)).toContainText('Ctrl+S');
      await page.keyboard.press('Escape');
      await expect(
        page.locator('[data-tour="task-form.title"]').first(),
      ).toBeHidden();
      expect(
        writes
          .filter((w) => w.path === 'onboarding/tours/start.first-task')
          .at(-1)?.body.status,
      ).not.toBe('paused');
      await page.locator('body').click({ position: { x: 1, y: 1 } });
      await page.keyboard.press('Escape');
      await expect(popover(page)).toBeHidden();
      await expect
        .poll(
          () =>
            writes
              .filter((w) => w.path === 'onboarding/tours/start.first-task')
              .at(-1)?.body,
        )
        .toMatchObject({ status: 'paused', step_id: 's2' });
      await page.goto('/demo/getting-started');
      const row = page.locator('[data-tour-id="start.first-task"]');
      await expect(row.getByRole('button', { name: /Resume/ })).toBeVisible();
      await expect(row).toContainText('step 2 of 3');
      await shoot(page, 'hub-resume', SIZES[0], 'light');
    },
  );

  engine(
    'G1 runs from the keyboard alone and focus stays inside the popover',
    async ({ page }) => {
      await boot(page, SIZES[0]);
      await page.goto('/demo/getting-started');
      const start = page.getByTestId('hub-start-start.nav');
      await start.focus();
      await page.keyboard.press('Enter');
      await expect(popover(page)).toBeVisible();
      await expect.poll(() => insideDialog(page)).toBe(true);
      for (let step = 0; step < 5; step++) {
        await expect(popover(page)).toBeVisible();
        for (let i = 0; i < 6; i++) {
          await page.keyboard.press('Tab');
          expect(await insideDialog(page)).toBe(true);
        }
        await page.keyboard.press('Shift+Tab');
        expect(await insideDialog(page)).toBe(true);
        const next = popover(page).getByRole('button', {
          name: /^(Next|Done)$/,
        });
        if (!(await next.count())) break;
        const label = await next.innerText();
        await next.focus();
        await page.keyboard.press('Enter');
        if (label === 'Done') break;
      }
      await expect(popover(page)).toBeHidden();
      await expect
        .poll(() => page.evaluate(() => document.activeElement?.tagName))
        .toBe('H1');
    },
  );

  engine(
    'T2 on a phone uses the status field and 44px sheet buttons',
    async ({ page }) => {
      await boot(page, SIZES[1]);
      await page.goto('/demo/getting-started');
      await page.getByTestId('hub-start-tasks.board-move').click();
      await expect(page).toHaveURL(/\/demo\/board$/);
      const sheet = popover(page);
      await expect(sheet).toContainText('Columns');
      const box = await sheet.boundingBox();
      expect(box.y + box.height).toBeGreaterThan(SIZES[1].height - 40);
      await assertTouchTargets(sheet);
      await shoot(page, 't2-column', SIZES[1], 'light');
      await sheet.getByRole('button', { name: 'Next' }).click();
      await expect(sheet).toContainText('Open a card and change its Status');
      await page.locator('[data-task-id]').first().click();
      await expect(
        page.locator('[data-tour="task-form.status"]').first(),
      ).toBeVisible();
      await expect(page.locator('.tour-ring')).toBeVisible();
      await assertTouchTargets(sheet);
      await shoot(page, 't2-status', SIZES[1], 'light');
      await shoot(page, 't2-status', SIZES[1], 'dark');
    },
  );

  engine(
    'the skip menu pauses, skips and changes tips mode',
    async ({ page }) => {
      const { writes } = await boot(page, SIZES[0]);
      const last = (path) => writes.filter((w) => w.path === path).at(-1)?.body;
      const open = async () => {
        await page.goto('/demo/getting-started');
        await page
          .locator('[data-tour-id="start.nav"] button[data-testid^="hub-"]')
          .click();
        await expect(popover(page)).toContainText('Step ');
      };
      const choose = async (name) => {
        await page.waitForTimeout(600);
        await popover(page).getByRole('button', { name: 'Skip' }).click();
        await shoot(page, 'skip-menu', SIZES[0], 'light');
        await page.getByRole('menuitem', { name: new RegExp(name) }).click();
      };

      await open();
      await choose('Skip this step');
      await expect(popover(page)).toContainText('Step 2 of 5');

      await choose('Pause tour');
      await expect(popover(page)).toBeHidden();
      await expect
        .poll(() => last('onboarding/tours/start.nav'))
        .toMatchObject({ status: 'paused', step_id: 's2' });

      await open();
      await choose("I'll explore myself");
      await expect(popover(page)).toBeHidden();
      await expect
        .poll(() => last('onboarding/preferences'))
        .toEqual({ tips_mode: 'hub_only' });

      await open();
      await choose('Never show tips');
      await expect
        .poll(() => last('onboarding/preferences'))
        .toEqual({ tips_mode: 'off' });

      await open();
      await choose('Skip tour');
      await expect(popover(page)).toBeHidden();
      await expect
        .poll(() => last('onboarding/tours/start.nav'))
        .toMatchObject({ status: 'skipped' });
    },
  );

  engine(
    'with tips_mode hub_only or off no tour starts by itself',
    async ({ page }) => {
      for (const mode of ['hub_only', 'off']) {
        await boot(page, SIZES[0], { onboarding: { tips_mode: mode } });
        for (const url of [
          '/demo/list',
          '/demo/getting-started',
          '/demo/board',
        ]) {
          await page.goto(url);
          await page.waitForTimeout(1800);
          await expect(dialogs(page)).toHaveCount(0);
        }
      }
    },
  );

  engine(
    'with tips_mode auto the first visit of the hub starts the hub tour once',
    async ({ page }) => {
      await boot(page, SIZES[0], { onboarding: AUTO });
      await page.goto('/demo/getting-started');
      await expect(popover(page)).toContainText('Your Getting started page');
      await page.keyboard.press('Escape');
      await expect(popover(page)).toBeHidden();
      await page.goto('/demo/list');
      await page.waitForTimeout(1800);
      await expect(dialogs(page)).toHaveCount(0);
    },
  );

  engine(
    'first visit of a list that already has tasks does not start G2',
    async ({ page }) => {
      await boot(page, SIZES[0], {
        onboarding: { ...AUTO, facts: { has_task: true } },
      });
      await page.goto('/demo/list');
      await page.waitForTimeout(1800);
      await expect(dialogs(page)).toHaveCount(0);
    },
  );
});
