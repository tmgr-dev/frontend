import { mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { PLATFORMS, desktopPage } from './desktopShell.mjs';
import { mockOnboarding, setScheme } from './toursHelpers.mjs';

const e2e = process.env.S8_E2E === '1' ? test : test.skip;

const SHOTS =
  process.env.S8_SHOTS ??
  join(homedir(), 'projects/own-projects/tmgr.dev/docs/reports/tours/s8');
const ONBOARDING_KEY = 'tmgr:onboarding:v1';
const WHATS_NEW_VERSION = process.env.S8_WHATS_NEW_VERSION ?? '0.9.37';

test.use({ viewport: { width: 1280, height: 800 } });

const popover = (page) =>
  page.locator('[role="dialog"][aria-labelledby^="tour-"]').first();

const shoot = async (page, name) => {
  mkdirSync(SHOTS, { recursive: true });
  for (const scheme of ['light', 'dark']) {
    await setScheme(page, scheme);
    await page.screenshot({ path: join(SHOTS, `${name}-1280-${scheme}.png`) });
  }
  await setScheme(page, 'light');
};

const attached = async (page, anchors) => {
  for (const anchor of anchors)
    await expect(page.locator(`[data-tour="${anchor}"]`).first(), anchor).toBeAttached();
};

const trackTours = (page) =>
  page.addInitScript((key) => {
    window.__toursStarted = [];
    const set = Storage.prototype.setItem;
    Storage.prototype.setItem = function (k, v) {
      if (k === key) {
        try {
          for (const [id, state] of Object.entries(JSON.parse(v).tours ?? {}))
            if (state.status === 'in_progress' && !window.__toursStarted.includes(id))
              window.__toursStarted.push(id);
        } catch {}
      }
      return set.call(this, k, v);
    };
  }, ONBOARDING_KEY);

const localTour = (page, id) =>
  page.evaluate(
    ([key, tour]) => JSON.parse(localStorage.getItem(key) ?? '{}').tours?.[tour] ?? null,
    [ONBOARDING_KEY, id],
  );

const advance = async (page) => {
  const button = popover(page).getByRole('button', {
    name: /^(Next|Done|Got it|Finish|Skip step|Skip this step)$/,
  });
  const link = popover(page).getByText('Skip this step');
  if (await button.count()) await button.first().click();
  else if (await link.count()) await link.first().click();
};

const finishTour = async (page, shot) => {
  for (let i = 0; i < 12 && (await popover(page).count()); i++) {
    if (shot) {
      const title = await popover(page).locator('[id^="tour-title"]').first().innerText();
      await shoot(page, `${shot}-${title.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`);
    }
    await advance(page);
    await page.waitForTimeout(300);
  }
};

const localBoot = async (page, { platform = null, profile = null } = {}) => {
  const shell = await desktopPage(page, { account: false }, { profile, platform });
  await trackTours(page);
  return shell;
};

const startLocal = async (page) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/welcome/);
  await page.getByTestId('start-local').click();
  await page.getByLabel('Your name').fill('Ada');
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
};

const answerConsent = async (page) => {
  const decline = page.locator('[data-consent="usage-decline"], [data-consent="decline"]');
  if (await decline.count()) await decline.first().click();
};

const profile = {
  id: -1,
  uuid: '00000000-0000-4000-8000-000000000001',
  name: 'Ada',
  created_at: '2026-10-11T10:00:00Z',
};

const cloudBoot = async (page, { platform = null, seed = {}, shell = {} } = {}) => {
  const fixture = await mockOnboarding(page, { tips_mode: 'hub_only', ...seed });
  const desktop = await desktopPage(page, { multiStatus: true }, { platform, ...shell });
  await trackTours(page);
  return { fixture, desktop };
};

const startFromHub = async (page, id, route = '/demo/getting-started') => {
  await page.goto(route);
  await page.getByTestId(`hub-start-${id}`).click();
  await expect(popover(page)).toBeVisible();
};

const runTimer = async (page) => {
  await page.getByText('Try a timer on this task').first().click();
  const timer = page.locator('[data-tour="task-form.timer"] button').first();
  await timer.click();
  const confirm = page.getByRole('button', { name: 'Ok', exact: true });
  await confirm.click();
  await page.waitForTimeout(1200);
  await timer.click();
  await page.keyboard.press('Escape');
};

e2e('fresh local profile: welcome, starters, then D5, G2, TI1, D1 in order', async ({ page }) => {
  await localBoot(page);
  await startLocal(page);
  await answerConsent(page);
  for (const title of ['Plan your week', 'Try a timer on this task', 'Drag me to In progress'])
    await expect(page.getByText(title).first()).toBeVisible();

  await expect(popover(page)).toBeVisible();
  await shoot(page, 'first-run-d5');
  const order = ['desktop.local-workspace', 'start.first-task', 'time.timer', 'desktop.quick-add'];
  for (const [index, id] of order.entries()) {
    await expect
      .poll(() => page.evaluate(() => window.__toursStarted), { timeout: 20000 })
      .toContain(id);
    expect(await page.evaluate((tourId) => window.__toursStarted.indexOf(tourId), id)).toBe(index);
    if (id === 'time.timer') await runTimer(page);
    await finishTour(page, 'first-run');
  }

  await page.getByRole('link', { name: 'Board', exact: true }).first().click();
  await expect(page.getByTestId('remove-sample-tasks').first()).toBeVisible();
  await shoot(page, 'starters-board');
  await page.getByTestId('remove-sample-tasks').first().click();
  await expect(page.getByText('Plan your week')).toHaveCount(0);
});

e2e('every local anchor exists in the DOM', async ({ page }) => {
  await localBoot(page, { profile });
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await attached(page, ['local.badge', 'nav.create-account']);
  await page.goto('/local-my-workspace/board');
  await page.evaluate(() => localStorage.removeItem('tmgr:local-banner-dismissed'));
  await expect(page.locator('[data-tour="local.banner"]').first()).toBeAttached();

  await page.goto('/settings?tab=desktop');
  await attached(page, ['settings.desktop-shortcuts']);
  await page.goto('/settings?tab=data');
  await attached(page, ['settings.data-backups', 'settings.backup-now', 'settings.data-move']);
  await page.goto('/settings/plugins');
  await attached(page, ['plugins.install', 'plugins.help']);
  await page.goto('/settings/personas');
  await attached(page, ['local-personas.panel', 'local-personas.access', 'local-personas.connect']);
  await shoot(page, 'local-anchors');
});

e2e('open-window anchors exist for a saved task and a page', async ({ page }) => {
  await cloudBoot(page);
  await page.goto('/demo/board');
  await page.getByText('Original task').first().click();
  await expect(page.getByPlaceholder('Task name').first()).toHaveValue('Original task');
  await attached(page, ['task-form.open-window']);
  await page.goto('/demo/pages');
  const row = page.locator('a[href*="/pages/"]').first();
  if (await row.count()) {
    await row.click();
    await expect(page.getByTestId('page-open-window')).toHaveAttribute(
      'data-tour',
      'task-form.open-window',
    );
  }
});

e2e('quick-add window relays created, main completes D1 s2', async ({ page, browser }) => {
  await localBoot(page, { profile });
  await page.goto('/local-my-workspace/getting-started');
  await page.getByTestId('hub-start-desktop.quick-add').click();
  await expect(popover(page)).toBeVisible();
  await advance(page);
  await expect(popover(page)).toContainText(/routine|task/i);
  await page.evaluate(() => window.__emit('quick-add://created', { kind: 'routine' }));
  await expect.poll(() => localTour(page, 'desktop.quick-add').then((s) => s?.status)).toBe('completed');

  const other = await browser.newPage();
  await desktopPage(other, {}, { windowLabel: 'quick-add' });
  await other.route('**/api/daily-routines/tasks', (route) =>
    route.fulfill({ json: { data: { id: 5, workspace_id: 1 } } }),
  );
  await other.goto('/quick-add');
  await other.locator('form input, form textarea').first().fill('Stretch');
  await other.keyboard.press('Enter');
  await expect
    .poll(() => other.evaluate(() => (window.__emitted ?? []).map((e) => e.event)))
    .toContain('quick-add://created');
  const emitted = await other.evaluate(() => window.__emitted.find((e) => e.event === 'quick-add://created'));
  expect(JSON.stringify(emitted.payload)).not.toContain('Stretch');
  await other.close();
});

for (const [name, platform, expected] of [
  ['windows', PLATFORMS.windows, /system tray/i],
  ['linux', PLATFORMS.linux, /system tray/i],
]) {
  e2e(`D2 capture and D3 timer copy on ${name}`, async ({ page }) => {
    await cloudBoot(page, { platform });
    await startFromHub(page, 'desktop.capture');
    const body = popover(page).locator('[id^="tour-body"]');
    await expect(body).not.toContainText(/selection/i);
    await expect(body).toContainText(/screenshot/i);
    await shoot(page, `d2-${name}`);
    await page.goto('/settings?tab=desktop');
    await attached(page, ['settings.desktop-shortcuts']);

    await startFromHub(page, 'desktop.menu-bar-timer');
    await expect(popover(page)).toContainText(expected);
    await shoot(page, `d3-${name}`);
  });
}

e2e('D3 on macOS speaks of the menu bar', async ({ page }) => {
  await cloudBoot(page);
  await startFromHub(page, 'desktop.menu-bar-timer');
  await expect(popover(page)).toContainText(/menu bar/i);
});

e2e('D6 completes on Back up now', async ({ page }) => {
  await localBoot(page, { profile });
  await page.goto('/local-my-workspace/getting-started');
  await page.getByTestId('hub-start-desktop.backups').click();
  await expect(popover(page)).toBeVisible();
  await advance(page);
  await expect(page).toHaveURL(/tab=data/);
  await attached(page, ['settings.data-backups', 'settings.backup-now']);
  await shoot(page, 'd6-backups');
  await page.locator('[data-tour="settings.backup-now"]').first().click();
  await expect.poll(() => localTour(page, 'desktop.backups').then((s) => s?.status)).toBe('completed');
});

e2e('D7 is not startable on a Windows UA', async ({ page }) => {
  await localBoot(page, { profile, platform: PLATFORMS.windows });
  await page.goto('/local-my-workspace/getting-started');
  await expect(page.getByTestId('hub-start-desktop.local-agent')).toHaveCount(0);
});

e2e('D7 runs on macOS up to the connect anchor', async ({ page }) => {
  await localBoot(page, { profile });
  await page.goto('/local-my-workspace/getting-started');
  await page.getByTestId('hub-start-desktop.local-agent').click();
  await expect(popover(page)).toBeVisible();
  await advance(page);
  await attached(page, ['local-personas.panel', 'local-personas.access']);
  await expect(popover(page)).toBeInViewport({ ratio: 1 });
  await shoot(page, 'd7-agent');
});

e2e('D8 starts on the first visit to plugins', async ({ page }) => {
  await cloudBoot(page, { seed: { tips_mode: 'auto' } });
  await page.goto('/settings/plugins');
  await expect(popover(page)).toBeVisible();
  await attached(page, ['plugins.install', 'plugins.help']);
  await expect(popover(page)).toContainText('Plugins');
  await shoot(page, 'd8-plugins');
});

e2e('D9 button opens the tour from What is new', async ({ page }) => {
  await cloudBoot(page, {
    shell: { appVersion: WHATS_NEW_VERSION, lastSeenVersion: null },
  });
  await page.goto('/demo/board');
  const modal = page.getByTestId('whats-new-modal');
  await expect(modal).toBeVisible();
  const button = modal.locator('[data-tour="whats-new.item-tour"]').first();
  await expect(button).toBeVisible();
  await expect(button).toContainText('Take the tour:');
  await shoot(page, 'd9-whats-new');
  await button.click();
  await expect(modal).toHaveCount(0);
  await expect(popover(page)).toBeVisible();
});

e2e('D10 starts from the hub on the data tab and waits for the move', async ({ page }) => {
  await cloudBoot(page);
  await startFromHub(page, 'desktop.move-to-cloud');
  await page.evaluate(() =>
    window.__TAURI_INTERNALS__.invoke('local_workspace_create', { name: 'Home' }),
  );
  await page.goto('/settings?tab=data');
  await attached(page, ['settings.data-move']);
  await shoot(page, 'd10-move');
});
