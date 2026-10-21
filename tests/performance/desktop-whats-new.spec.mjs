import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';
import { mockOnboarding } from './toursHelpers.mjs';

const KEY = 'desktop.whatsNew.lastSeenVersion';
const lastSeen = (page) =>
  page.evaluate((key) => localStorage.getItem(key), KEY);

const noAutoTours = (page) =>
  page.addInitScript(() => sessionStorage.setItem('tmgr:tours:auto', 'true'));

const taskModalId = (page) =>
  page.evaluate(
    () =>
      document.querySelector('#app').__vue_app__.config.globalProperties.$store
        .state.currentTaskIdForModal,
  );

const tourPopover = (page) =>
  page.locator('[role="dialog"][aria-labelledby^="tour-"]');

const watchCloses = (page) =>
  page.addInitScript((key) => {
    window.__lastSeenWrites = 0;
    window.__emptyFlash = false;
    const set = Storage.prototype.setItem;
    Storage.prototype.setItem = function (k, v) {
      if (k === key) window.__lastSeenWrites++;
      return set.call(this, k, v);
    };
    new MutationObserver(() => {
      if (
        document.body?.textContent?.includes(
          'No release notes for this version.',
        )
      )
        window.__emptyFlash = true;
    }).observe(document, { childList: true, subtree: true });
  }, KEY);

const mockPromotion = (page) =>
  page.route('**/api/promotions**', (route) =>
    route.fulfill({
      json: {
        data:
          route.request().method() === 'GET'
            ? [
                {
                  id: 7,
                  product: 'Notes',
                  title: 'Private notes app',
                  text: 'Sync notes across devices.',
                  icon_url: null,
                  url: 'https://example.com/notes',
                  placement: 'release_notes',
                },
              ]
            : {},
      },
    }),
  );

test('an update shows every skipped release newest first and remembers it', async ({
  page,
}) => {
  await desktopPage(
    page,
    {},
    { appVersion: '0.9.8', lastSeenVersion: '0.9.6' },
  );
  await page.goto('/demo/board');
  const modal = page.getByTestId('whats-new-modal');
  await expect(modal).toBeVisible();
  await expect(modal.getByRole('heading', { level: 3 })).toHaveText([
    'v0.9.8',
    'v0.9.7',
  ]);
  await expect(modal.locator('li').first()).toBeVisible();
  await expect(modal.locator('li br')).toHaveCount(0);

  const before = page.url();
  await modal.getByRole('button', { name: 'Got it' }).click();
  await expect(modal).toHaveCount(0);
  expect(page.url()).toBe(before);
  expect(await lastSeen(page)).toBe('0.9.8');

  await page.reload();
  await expect(page.getByText('Backlog').first()).toBeVisible();
  await expect(page.getByTestId('whats-new-modal')).toHaveCount(0);
});

test('an existing user without the key sees only the current release', async ({
  page,
}) => {
  await desktopPage(page, {}, { appVersion: '0.9.8', lastSeenVersion: null });
  await page.goto('/demo/board');
  const modal = page.getByTestId('whats-new-modal');
  await expect(modal.getByRole('heading', { level: 3 })).toHaveText(['v0.9.8']);
  await expect(modal.getByRole('heading', { level: 2 })).toHaveText(
    "What's new in 0.9.8",
  );
});

test('a logged-out desktop stays silent until sign-in', async ({ page }) => {
  await desktopPage(page, {}, { appVersion: '0.9.8', lastSeenVersion: null });
  await page.addInitScript(() => {
    if (!sessionStorage.getItem('fixture.fresh')) {
      sessionStorage.setItem('fixture.fresh', '1');
      localStorage.removeItem('token');
    }
  });
  await page.goto('/login');
  await expect.poll(() => lastSeen(page)).toBeNull();
  await expect(page.getByTestId('whats-new-modal')).toHaveCount(0);
});

test('the status bar version reopens the notes and ESC closes them', async ({
  page,
}) => {
  await desktopPage(
    page,
    {},
    { appVersion: '0.9.8', lastSeenVersion: '0.9.8' },
  );
  await page.goto('/demo/board');
  const modal = page.getByTestId('whats-new-modal');
  await expect(page.getByText('Backlog').first()).toBeVisible();
  await expect(modal).toHaveCount(0);

  await page.getByRole('button', { name: 'v0.9.8' }).click();
  await expect(modal).toBeVisible();
  const headings = await modal
    .getByRole('heading', { level: 3 })
    .allTextContents();
  expect(headings[0]).toBe('v0.9.8');
  expect(headings.length).toBeGreaterThan(1);

  await page.keyboard.press('Escape');
  await expect(modal).toHaveCount(0);
});

test('a plain web page never shows the modal', async ({ page }) => {
  await mockApp(page);
  await page.goto('/demo/board');
  await expect(page.getByText('Backlog').first()).toBeVisible();
  await expect(page.getByTestId('whats-new-modal')).toHaveCount(0);
});

const THEMES = [
  { name: 'light', colorScheme: 'default', theme: 'default' },
  { name: 'dark', colorScheme: 'dark', theme: 'default' },
  { name: 'dracula', colorScheme: 'dark', theme: 'dracula' },
  { name: 'github-light', colorScheme: 'default', theme: 'github-light' },
];

for (const theme of THEMES) {
  test(`the panel and the promo card follow the ${theme.name} theme tokens`, async ({
    page,
  }) => {
    await page.addInitScript((t) => {
      localStorage.setItem('colorScheme', t.colorScheme);
      localStorage.setItem('theme', t.theme);
    }, theme);
    await noAutoTours(page);
    await desktopPage(
      page,
      {},
      { appVersion: '0.9.8', lastSeenVersion: '0.9.6' },
    );
    await mockPromotion(page);
    await page.goto('/demo/board');
    const modal = page.getByTestId('whats-new-modal');
    await expect(modal.getByTestId('promotion-card')).toBeVisible();
    const colors = await page.evaluate(() => {
      const token = (name) => {
        const probe = document.createElement('div');
        probe.style.background = `hsl(var(${name}))`;
        document.body.appendChild(probe);
        const value = getComputedStyle(probe).backgroundColor;
        probe.remove();
        return value;
      };
      const bg = (el) => getComputedStyle(el).backgroundColor;
      const root = document.querySelector('[data-testid="whats-new-modal"]');
      return {
        panel: bg(root.closest('[role="dialog"]')),
        card: bg(root.querySelector('[data-testid="promotion-card"]')),
        background: token('--background'),
        cardToken: token('--card'),
      };
    });
    expect(colors.panel).toBe(colors.background);
    expect(colors.panel).not.toBe('rgb(17, 24, 39)');
    expect(colors.card).toBe(colors.cardToken);

    const shots = process.env.WHATS_NEW_SHOTS;
    if (shots) {
      mkdirSync(shots, { recursive: true });
      await page.screenshot({
        path: join(shots, `whats-new-${theme.name}.png`),
      });
    }
  });
}

const CLOSE_PATHS = {
  'the X button': (page) =>
    page.getByRole('dialog').getByRole('button', { name: 'Close' }).click(),
  'Got it': (page) =>
    page
      .getByTestId('whats-new-modal')
      .getByRole('button', { name: 'Got it' })
      .click(),
  ESC: (page) => page.keyboard.press('Escape'),
  'an overlay click': (page) => page.mouse.click(1250, 400),
};

for (const [name, closeVia] of Object.entries(CLOSE_PATHS)) {
  test(`closing via ${name} remembers the version once and frees the board`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await noAutoTours(page);
    await desktopPage(
      page,
      {},
      { appVersion: '0.9.8', lastSeenVersion: '0.9.6' },
    );
    await watchCloses(page);
    await page.goto('/demo/board');
    const modal = page.getByTestId('whats-new-modal');
    await expect(modal).toBeVisible();
    const writes = () => page.evaluate(() => window.__lastSeenWrites);
    const before = await writes();

    await closeVia(page);
    await expect(modal).toHaveCount(0);
    await expect(page.getByRole('dialog')).toHaveCount(0);
    expect(await writes()).toBe(before + 1);
    expect(await lastSeen(page)).toBe('0.9.8');
    expect(await page.evaluate(() => window.__emptyFlash)).toBe(false);
    expect(
      await page.evaluate(() => document.body.style.pointerEvents),
    ).not.toBe('none');

    await page.getByText('Original task').first().click();
    await expect.poll(() => taskModalId(page)).not.toBeNull();
  });
}

test('ESC over an open task closes only the notes', async ({ page }) => {
  await noAutoTours(page);
  await desktopPage(
    page,
    {},
    { appVersion: '0.9.8', lastSeenVersion: '0.9.8' },
  );
  await page.goto('/demo/board');
  await page.getByText('Original task').first().click();
  await expect.poll(() => taskModalId(page)).not.toBeNull();

  // The task panel covers the status bar, so open the notes the way its
  // version button does.
  await page.evaluate(async () => {
    const { openWhatsNew } = await import('/src/utils/whatsNewState.ts');
    await openWhatsNew();
  });
  const modal = page.getByTestId('whats-new-modal');
  await expect(modal).toBeVisible();

  await page.keyboard.press('Escape');
  expect(await taskModalId(page)).not.toBeNull();
  await expect(modal).toHaveCount(0);
  expect(await taskModalId(page)).not.toBeNull();

  await page.keyboard.press('Escape');
  await expect.poll(() => taskModalId(page)).toBeNull();
});

test('a version without notes shows the empty state', async ({ page }) => {
  await noAutoTours(page);
  await desktopPage(
    page,
    {},
    { appVersion: '0.0.1', lastSeenVersion: '0.0.1' },
  );
  await page.goto('/demo/board');
  await expect(page.getByText('Backlog').first()).toBeVisible();
  await page.getByRole('button', { name: 'v0.0.1' }).click();
  await expect(
    page
      .getByTestId('whats-new-modal')
      .getByText('No release notes for this version.'),
  ).toBeVisible();
});

test('ESC with the auto What is new tour open does not leave the tour behind', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockOnboarding(page);
  await desktopPage(
    page,
    { multiStatus: true },
    { appVersion: '0.9.37', lastSeenVersion: null },
  );
  await page.goto('/demo/board');
  await expect(page.getByTestId('whats-new-modal')).toBeVisible();
  await expect(tourPopover(page)).toBeVisible();

  await page.keyboard.press('Escape');
  await expect(tourPopover(page)).toHaveCount(0);
  await expect(page.locator('.driver-overlay')).toHaveCount(0);
  await expect(page.getByTestId('whats-new-modal')).toBeVisible();

  await page.keyboard.press('Escape');
  await expect(page.getByTestId('whats-new-modal')).toHaveCount(0);
});
