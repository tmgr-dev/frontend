import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

test.use({ storageState: { cookies: [], origins: [] } });

const screensDir = process.env.SCREENS_DIR;
if (screensDir) mkdirSync(screensDir, { recursive: true });
const shot = async (page, name) => {
  if (screensDir) await page.screenshot({ path: join(screensDir, name) });
};

const TRACKERS = /google-analytics|googletagmanager|analytics\.google|yandex/i;

const watchTelemetry = async (context) => {
  const events = [];
  const others = [];
  await context.route(/analytics\.dzik/, (route) =>
    route.fulfill({ status: 200, contentType: 'text/javascript', body: '' }),
  );
  await context.route(TRACKERS, (route) => {
    others.push(route.request().url());
    return route.fulfill({ status: 204, body: '' });
  });
  const route = async (page) =>
    page.route('**/api/t/**', async (handler) => {
      const request = handler.request();
      events.push({
        url: request.url(),
        method: request.method(),
        headers: await request.allHeaders(),
        body: request.postData(),
      });
      await handler.fulfill({ status: 204, body: '' });
    });
  return { events, others, route };
};

const goHidden = (page) =>
  page.evaluate(() => {
    Object.defineProperty(document, 'visibilityState', {
      value: 'hidden',
      configurable: true,
    });
    document.dispatchEvent(new Event('visibilitychange'));
  });

const shareUsage = async (page) => {
  await page.getByRole('button', { name: 'Share usage data' }).click();
  await expect(page.locator('[data-consent-block="usage"]')).toHaveCount(0);
};

test('a fresh visit sends nothing to telemetry or trackers before any choice', async ({
  page,
}) => {
  await mockApp(page, { account: false, consent: false });
  const { events, others, route } = await watchTelemetry(page.context());
  await route(page);
  await page.goto('/login');
  await expect(page.locator('[data-consent-block="usage"]')).toBeVisible();
  await goHidden(page);
  await page.waitForTimeout(800);
  expect(events).toEqual([]);
  expect(others).toEqual([]);
  const stored = await page.evaluate(() =>
    Object.keys(localStorage).filter((key) => key.startsWith('tmgr:telemetry')),
  );
  expect(stored).toEqual([]);
});

test('both blocks stand alone and the buttons weigh the same', async ({ page }) => {
  await mockApp(page, { account: false, consent: false });
  await page.goto('/login');
  const no = page.getByRole('button', { name: 'No, thanks' });
  const yes = page.getByRole('button', { name: 'Share usage data' });
  expect(await no.getAttribute('class')).toBe(await yes.getAttribute('class'));
  const [a, b] = [await no.boundingBox(), await yes.boundingBox()];
  expect(a.width).toBeCloseTo(b.width, 0);
  expect(a.height).toBeCloseTo(b.height, 0);
  await expect(page.getByRole('link', { name: 'What we collect' })).toHaveAttribute(
    'href',
    '/privacy#usage-data',
  );
  await no.click();
  await expect(page.locator('[data-consent-block="usage"]')).toHaveCount(0);
  await expect(page.locator('[data-consent-block="analytics"]')).toBeVisible();
  await page.getByRole('button', { name: 'Decline' }).click();
  await expect(page.getByRole('region', { name: 'Privacy choices' })).toHaveCount(0);
});

test('sharing sends a credential-free batch and no id exists before it', async ({
  page,
}) => {
  await mockApp(page, { account: true, consent: false });
  const { events, route } = await watchTelemetry(page.context());
  await route(page);
  await page.goto('/login');
  expect(
    await page.evaluate(() => localStorage.getItem('tmgr:telemetry:identity')),
  ).toBeNull();
  await shareUsage(page);
  await goHidden(page);
  await expect.poll(() => events.length).toBeGreaterThan(0);
  const [first] = events;
  expect(first.method).toBe('POST');
  expect(new URL(first.url).pathname).toBe('/api/t/events');
  expect(first.headers.authorization).toBeUndefined();
  expect(first.headers.cookie).toBeUndefined();
  const body = JSON.parse(first.body);
  expect(body.schema).toBe(1);
  expect(body.secret_sha256).toMatch(/^[0-9a-f]{64}$/);
  expect(body.events.map((e) => e.n)).toContain('app_opened');
});

test('saying no sends nothing, also after a reload', async ({ page }) => {
  await mockApp(page, { account: true, consent: false });
  const { events, route } = await watchTelemetry(page.context());
  await route(page);
  await page.goto('/login');
  await page.getByRole('button', { name: 'No, thanks' }).click();
  await page.getByRole('button', { name: 'Decline' }).click();
  await page.reload();
  await goHidden(page);
  await page.waitForTimeout(800);
  expect(events).toEqual([]);
  expect(
    await page.evaluate(() => localStorage.getItem('tmgr:telemetry:identity')),
  ).toBeNull();
});

test('turning sharing off in Settings stops a second tab at once', async ({
  context,
  page,
}) => {
  await mockApp(page, { account: true, consent: false });
  const { events, route } = await watchTelemetry(context);
  await route(page);
  await page.goto('/login');
  await shareUsage(page);
  await page.getByRole('button', { name: 'Decline' }).click();

  const second = await context.newPage();
  await mockApp(second, { account: true, consent: false });
  await route(second);
  await second.goto('/login');
  await expect(second.locator('[data-consent-block="usage"]')).toHaveCount(0);
  await second.waitForTimeout(1000);
  await goHidden(second);
  await expect.poll(() => events.length).toBeGreaterThan(0);
  const before = events.length;

  await page.goto('/settings?tab=privacy');
  await expect(page.getByTestId('usage-switch')).toBeVisible();
  await page.getByTestId('usage-switch').click();
  await expect(page.getByTestId('install-id')).toHaveText('None while sharing is off');
  await expect
    .poll(() => second.evaluate(() => localStorage.getItem('tmgr:telemetry:identity')))
    .toBeNull();

  await second.evaluate(() => (window.__tmgrBefore = 0));
  await goHidden(second);
  await second.waitForTimeout(800);
  expect(events.length).toBe(before);
  const receipts = await page.evaluate(() =>
    JSON.parse(localStorage.getItem('tmgr:telemetry:receipts') ?? '[]'),
  );
  expect(receipts).toHaveLength(1);
});

test.describe('settings and screenshots', () => {
  test('the Privacy tab shows the controls and deletes sent data', async ({
    page,
  }) => {
    await mockApp(page, { account: true, consent: false });
    const { events, route } = await watchTelemetry(page.context());
  await route(page);
    await page.goto('/settings?tab=privacy');
    await shareUsage(page);
    await page.getByRole('button', { name: 'Decline' }).click();
    await expect(page.getByText('Not linked to your account:')).toBeVisible();
    await expect(page.getByTestId('pending-count')).toHaveText(/^[1-9]/);
    await expect(page.getByTestId('install-id')).toHaveText(/^[0-9a-f]{4}…[0-9a-f]{4}$/);
    await expect(page.getByText('Turning sharing off stops it at once.')).toBeVisible();
    await expect(page.getByRole('link', { name: 'Read more' })).toHaveAttribute(
      'href',
      '/privacy#onboarding-aggregates',
    );
    await shot(page, 's6b-settings-privacy.png');
    await page.getByRole('button', { name: 'View' }).click();
    await expect(page.getByTestId('queue-json')).toContainText('app_opened');
    await page.keyboard.press('Escape');
    await goHidden(page);
    await expect.poll(() => events.some((e) => e.method === 'POST')).toBe(true);
    await page.getByRole('button', { name: 'Delete data already sent' }).click();
    await expect(page.getByTestId('delete-result')).toHaveText('Deleted. Sharing is off.');
    expect(events.some((e) => e.method === 'DELETE' && e.headers['x-telemetry-secret'])).toBe(
      true,
    );
    await expect(page.getByTestId('usage-switch')).not.toBeChecked();
  });

  for (const [name, width, height] of [['1280', 1280, 800], ['390', 390, 844]]) {
    test(`consent panel at ${name}`, async ({ page }) => {
      await page.setViewportSize({ width, height });
      await mockApp(page, { account: false, consent: false });
      await page.goto('/login');
      await expect(page.getByRole('region', { name: 'Privacy choices' })).toBeVisible();
      await shot(page, `s6b-consent-panel-${name}.png`);
    });
  }
});

test('desktop without an account asks only about usage data and reports local mode', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await desktopPage(
    page,
    { account: false, consent: false },
    { profile: null },
  );
  const { events, others, route } = await watchTelemetry(page.context());
  await route(page);
  await page.goto('/');
  await expect(page).toHaveURL(/\/welcome/);
  await page.getByTestId('start-local').click();
  await page.getByLabel('Your name').fill('Ada');
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await expect(page.locator('[data-consent-block="usage"]')).toBeVisible();
  await expect(page.locator('[data-consent-block="analytics"]')).toHaveCount(0);
  expect(events).toEqual([]);
  await shareUsage(page);
  await goHidden(page);
  await expect.poll(() => events.length).toBeGreaterThan(0);
  const body = JSON.parse(events[0].body);
  expect(body.account_mode).toBe('local');
  expect(body.platform).toMatch(/^desktop_/);
  expect(events[0].headers.authorization).toBeUndefined();
  await page.goto('/settings?tab=privacy');
  await expect(page.getByTestId('usage-switch')).toBeVisible();
  await expect(page.getByTestId('analytics-switch')).toHaveCount(0);
  expect(others).toEqual([]);
});
