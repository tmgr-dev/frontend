import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

const screensDir = process.env.SCREENS_DIR;
if (screensDir) mkdirSync(screensDir, { recursive: true });
const shot = async (page, name) => {
  if (screensDir)
    await page.screenshot({ path: join(screensDir, name), fullPage: true });
};

const FOLDERS = [
  { tool: 'claude', label: 'claude', path: '~/.claude' },
  { tool: 'codex', label: 'codex', path: '~/.codex' },
];

const totals = {
  cost_usd: 1.25,
  tokens_total: 120000,
  input: 100000,
  output: 20000,
  cache_read: 0,
  cache_write: 0,
  reasoning: 0,
  events: 12,
  unpriced_events: 0,
};
const sums = { cost_usd: 0, tokens_total: 0, events: 0, unpriced_events: 0 };

const fakeUsage = (initial = {}) => {
  const calls = [];
  const replies = {};
  let consent = {
    supported: true,
    unsupported_reason: null,
    state: 'unset',
    claude: true,
    codex: true,
    decided_at: null,
    folders: FOLDERS,
    has_data: false,
    ...initial,
  };
  let events = 0;
  const handler = (command, args) => {
    calls.push({ command, args });
    if (replies[command]) throw replies[command];
    switch (command) {
      case 'usage_consent':
        return consent;
      case 'usage_consent_set': {
        const off = args.state === 'granted' && !args.claude && !args.codex;
        consent = {
          ...consent,
          state: off ? 'denied' : args.state,
          claude: args.claude,
          codex: args.codex,
          decided_at: new Date().toISOString(),
        };
        return consent;
      }
      case 'usage_forget':
        events = 0;
        consent = { ...consent, state: 'denied', has_data: false };
        return consent;
      case 'usage_scan':
        if (consent.state !== 'granted')
          return {
            state: 'off',
            files: 0,
            events_new: 0,
            events_updated: 0,
            errors: 0,
            bad_lines: 0,
          };
        events = 12;
        consent = { ...consent, has_data: true };
        return {
          state: 'done',
          files: 2,
          events_new: 12,
          events_updated: 0,
          errors: 0,
          bad_lines: 0,
        };
      case 'usage_overview':
        return {
          supported: true,
          unsupported_reason: null,
          has_data: events > 0,
          history_floor: null,
          accounts: [],
          windows: ['today', 'week', 'month'].map((key) => ({
            key,
            label: key[0].toUpperCase() + key.slice(1),
            from: '2026-10-01T00:00:00Z',
            to: '2026-10-25T00:00:00Z',
            totals,
            rows: [],
          })),
          days: [],
          limits: [],
        };
      case 'usage_tasks':
        return {
          tasks: [],
          ambiguous: sums,
          unattributed: sums,
          runs_used: 0,
          runs_dropped: 0,
          runs_truncated: false,
        };
      default:
        return null;
    }
  };
  return Object.assign(handler, {
    calls,
    replies,
    commands: () => calls.map((c) => c.command),
  });
};

const runner = (command) =>
  command === 'runner_enabled'
    ? true
    : command === 'runner_status'
      ? { state: 'ready', version: '0.4.0', caps: [] }
      : null;

const openUsage = async (page, usage, scheme = 'light', shell = {}) => {
  await page.addInitScript(
    (s) => localStorage.setItem('colorScheme', s),
    scheme,
  );
  await desktopPage(
    page,
    { onboarding: { tips_mode: 'off' } },
    { usage, ...shell },
  );
  await page.goto('/ai-usage');
  await expect(page).toHaveURL(/\/ai-usage$/);
};

test('first open asks before any log is read, Allow then scans', async ({
  page,
}) => {
  const usage = fakeUsage();
  await openUsage(page, usage, 'light', { runner });
  const screen = page.getByTestId('usage-consent');
  await expect(screen).toBeVisible();
  await expect(page.getByTestId('autopilot-panel')).toBeVisible();
  await expect(screen).toContainText('~/.claude');
  await expect(screen).toContainText('~/.codex');
  await expect(screen).toContainText(
    'What you or the AI wrote is never stored',
  );
  await page.waitForTimeout(300);
  expect(usage.commands()).not.toContain('usage_scan');
  expect(usage.commands()).not.toContain('usage_overview');
  await shot(page, 'consent-light.png');

  await page.getByTestId('usage-consent-codex').uncheck();
  await page.getByRole('button', { name: 'Allow', exact: true }).click();
  await expect(page.getByText('$1.25').first()).toBeVisible();
  const order = usage.commands();
  expect(order.indexOf('usage_consent_set')).toBeLessThan(
    order.indexOf('usage_scan'),
  );
  expect(
    usage.calls.find((c) => c.command === 'usage_consent_set').args,
  ).toEqual({ state: 'granted', claude: true, codex: false });
  await expect(screen).toHaveCount(0);
});

test('consent screen in dark mode', async ({ page }) => {
  const usage = fakeUsage({ has_data: true });
  await openUsage(page, usage, 'dark');
  await expect(page.getByTestId('usage-consent')).toBeVisible();
  expect(
    await page.evaluate(() =>
      document.documentElement.classList.contains('dark'),
    ),
  ).toBe(true);
  await expect(page.getByTestId('usage-consent')).toContainText(
    'Numbers TMGR read earlier stay on this device',
  );
  await shot(page, 'consent-dark.png');
});

test('Not now shows the off card and never scans; Allow there scans', async ({
  page,
}) => {
  const usage = fakeUsage();
  await openUsage(page, usage, 'light', { runner });
  await page.getByRole('button', { name: 'Not now' }).click();
  const off = page.getByTestId('usage-consent-off');
  await expect(off).toContainText('Reading AI tool logs is off');
  await expect(page.getByTestId('autopilot-panel')).toBeVisible();
  await expect(
    off.getByRole('link', { name: 'Settings → Privacy' }),
  ).toBeVisible();
  expect(usage.commands()).not.toContain('usage_scan');
  await shot(page, 'off-card.png');

  await page.reload();
  await expect(off).toBeVisible();
  expect(usage.commands()).not.toContain('usage_scan');

  await off.getByRole('button', { name: 'Allow' }).click();
  await expect(page.getByText('$1.25').first()).toBeVisible();
  expect(usage.commands()).toContain('usage_scan');
});

test('a consent read error shows Retry', async ({ page }) => {
  const usage = fakeUsage();
  usage.replies.usage_consent = 'Usage data is not readable.';
  await openUsage(page, usage);
  const alert = page.getByRole('alert').filter({
    hasText: 'Usage data is not readable.',
  });
  await expect(alert).toBeVisible();
  delete usage.replies.usage_consent;
  await alert.getByRole('button', { name: 'Retry' }).click();
  await expect(page.getByTestId('usage-consent')).toBeVisible();
  expect(usage.commands()).not.toContain('usage_scan');
});

test('Snap shows Not available, no consent screen', async ({ page }) => {
  const usage = fakeUsage({
    supported: false,
    unsupported_reason: 'AI usage is not available in the Snap build.',
    folders: [],
  });
  const snap = (command, args) =>
    command === 'usage_overview'
      ? {
          supported: false,
          unsupported_reason: 'AI usage is not available in the Snap build.',
        }
      : usage(command, args);
  await openUsage(page, snap);
  await expect(
    page.getByRole('heading', { name: 'Not available' }),
  ).toBeVisible();
  await expect(page.getByTestId('usage-consent')).toHaveCount(0);
  expect(usage.commands()).not.toContain('usage_scan');
});

test('Settings → Privacy: switches, Forget behind a confirm', async ({
  page,
}) => {
  const usage = fakeUsage({ state: 'granted', has_data: true });
  await page.addInitScript(() => localStorage.setItem('colorScheme', 'light'));
  await desktopPage(page, { onboarding: { tips_mode: 'off' } }, { usage });
  await page.goto('/settings?tab=privacy');
  const section = page.getByTestId('ai-usage-settings');
  await expect(section).toBeVisible();
  await expect(section).toContainText('~/.codex');
  if (screensDir)
    await section.screenshot({ path: join(screensDir, 'settings-light.png') });

  await page.getByTestId('usage-codex-switch').click();
  await expect(page.getByTestId('usage-codex-switch')).toHaveAttribute(
    'data-state',
    'unchecked',
  );
  expect(usage.calls.at(-1)).toEqual({
    command: 'usage_consent_set',
    args: { state: 'granted', claude: true, codex: false },
  });

  usage.replies.usage_forget = 'A scan is running. Try again in a moment.';
  await page.getByTestId('usage-forget').click();
  await page.getByRole('button', { name: 'Forget', exact: true }).click();
  await expect(page.getByTestId('usage-settings-error')).toHaveText(
    'A scan is running. Try again in a moment.',
  );
  delete usage.replies.usage_forget;

  await page.getByTestId('usage-forget').click();
  await expect(
    page
      .getByRole('alertdialog')
      .getByText("AI tools' own logs are not touched"),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  expect(usage.commands().filter((c) => c === 'usage_forget')).toHaveLength(1);

  await page.getByTestId('usage-forget').click();
  await page.getByRole('button', { name: 'Forget', exact: true }).click();
  await expect(page.getByTestId('usage-forget-result')).toHaveText(
    'Deleted. Reading AI tool logs is off.',
  );
  await expect(page.getByTestId('usage-read-switch')).toHaveAttribute(
    'data-state',
    'unchecked',
  );
  await expect(page.getByTestId('usage-claude-switch')).toBeDisabled();

  await page.getByTestId('usage-read-switch').click();
  await expect(page.getByTestId('usage-read-switch')).toHaveAttribute(
    'data-state',
    'checked',
  );
  expect(usage.calls.at(-1).args).toEqual({
    state: 'granted',
    claude: true,
    codex: false,
  });
});

test('Settings tells an undecided device what still reads logs', async ({
  page,
}) => {
  const usage = fakeUsage();
  await desktopPage(page, { onboarding: { tips_mode: 'off' } }, { usage });
  await page.goto('/settings?tab=privacy');
  await expect(page.getByTestId('ai-usage-settings')).toContainText(
    "Not chosen yet: the desktop app and background jobs don't read them",
  );
  await expect(page.getByTestId('usage-read-switch')).toHaveAttribute(
    'data-state',
    'unchecked',
  );
  await expect(page.getByTestId('usage-codex-switch')).toBeDisabled();
});

test('the web build has no AI usage section', async ({ page }) => {
  await mockApp(page, { onboarding: { tips_mode: 'off' } });
  await page.goto('/settings?tab=privacy');
  await expect(page.getByRole('heading', { name: 'Usage data' })).toBeVisible();
  await expect(page.getByTestId('ai-usage-settings')).toHaveCount(0);
});
