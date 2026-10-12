import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const screensDir = process.env.SCREENS_DIR;
if (screensDir) mkdirSync(screensDir, { recursive: true });
const shot = async (page, name) => {
  if (screensDir) await page.screenshot({ path: join(screensDir, name) });
};

const TRACKERS = /google-analytics|googletagmanager|analytics\.google|yandex/i;

const watchTrackers = async (page) => {
  const hits = [];
  await page.context().route(/analytics\.dzik/, (route) =>
    route.fulfill({ status: 200, contentType: 'text/javascript', body: '' }),
  );
  await page.context().route(TRACKERS, (route) => {
    hits.push(route.request().url());
    const script = route.request().resourceType() === 'script';
    return route.fulfill(
      script
        ? { status: 200, contentType: 'text/javascript', body: '' }
        : { status: 204, body: '' },
    );
  });
  return hits;
};

const gtagScripts = (page) =>
  page.locator('script[src*="googletagmanager"]').count();

test.beforeEach(async ({ page }) => {
  await mockApp(page, { account: false, consent: false });
});

test('a first visit makes no analytics requests and shows the choice', async ({
  page,
}) => {
  const hits = await watchTrackers(page);
  await page.goto('/login');
  const panel = page.getByRole('region', { name: 'Privacy choices' });
  await expect(panel).toBeVisible();
  await page.goto('/privacy');
  await expect(page.getByRole('heading', { name: 'Privacy policy' })).toBeVisible();
  await expect(panel).toHaveCount(0);
  await page.waitForTimeout(500);
  expect(hits).toEqual([]);
  expect(await gtagScripts(page)).toBe(0);
});

test('the two buttons weigh the same', async ({ page }) => {
  await page.goto('/login');
  const decline = page.getByRole('button', { name: 'Decline' });
  const accept = page.getByRole('button', { name: 'Allow analytics' });
  await expect(decline).toBeVisible();
  expect(await decline.getAttribute('class')).toBe(
    await accept.getAttribute('class'),
  );
  const [a, b] = [await decline.boundingBox(), await accept.boundingBox()];
  expect(a.width).toBeCloseTo(b.width, 0);
  expect(a.height).toBeCloseTo(b.height, 0);
});

test('allowing analytics loads the Google tag, and revoking removes it', async ({
  page,
}) => {
  const hits = await watchTrackers(page);
  await page.goto('/login');
  await page.getByRole('button', { name: 'Allow analytics' }).click();
  await expect.poll(() => hits.some((url) => /googletagmanager/.test(url))).toBe(true);
  expect(await gtagScripts(page)).toBe(1);
  await page.getByRole('button', { name: 'No, thanks' }).click();
  await expect(page.getByRole('region', { name: 'Privacy choices' })).toHaveCount(0);

  await page.getByRole('button', { name: 'Cookie settings' }).click();
  await page.getByRole('button', { name: 'Decline' }).click();
  expect(await gtagScripts(page)).toBe(0);
});

test('declining keeps the tag away after a reload', async ({ page }) => {
  const hits = await watchTrackers(page);
  await page.goto('/login');
  await page.getByRole('button', { name: 'Decline' }).click();
  await page.getByRole('button', { name: 'No, thanks' }).click();
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Welcome back' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Privacy choices' })).toHaveCount(0);
  await page.waitForTimeout(500);
  expect(hits).toEqual([]);
  expect(await gtagScripts(page)).toBe(0);
});

test('the privacy page opens for a guest with every section', async ({ page }) => {
  await page.goto('/privacy#analytics');
  for (const id of [
    'summary', 'controller', 'account-data', 'desktop-local', 'usage-data',
    'analytics', 'onboarding-aggregates', 'third-parties', 'retention',
    'your-rights', 'changes', 'contact',
  ]) {
    await expect(page.locator(`#${id}`)).toHaveCount(1);
  }
  await expect(page.getByText('Last updated: 12 October 2026')).toBeVisible();
  await expect(page.locator('[data-todo="controller-name"]')).toBeVisible();
  await expect(page.locator('#contact [data-todo="contact-email"]')).toBeVisible();

  await page.getByRole('button', { name: 'Cookie settings' }).click();
  await expect(page.getByRole('region', { name: 'Privacy choices' })).toBeVisible();
  await shot(page, 'privacy-page.png');
});

test.describe('screenshots', () => {
  for (const [name, width, height] of [['1280', 1280, 800], ['390', 390, 844]]) {
    test(`consent panel at ${name}`, async ({ page }) => {
      await page.setViewportSize({ width, height });
      await page.goto('/login');
      await expect(page.getByRole('region', { name: 'Privacy choices' })).toBeVisible();
      await shot(page, `consent-panel-${name}.png`);
    });
  }
});
