import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

const KEY = 'desktop.whatsNew.lastSeenVersion';
const lastSeen = (page) =>
  page.evaluate((key) => localStorage.getItem(key), KEY);

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

  const shots = process.env.WHATS_NEW_SHOTS;
  if (shots) {
    mkdirSync(shots, { recursive: true });
    await page.screenshot({ path: join(shots, 'whats-new-light.png') });
    await page.evaluate(() => {
      document.documentElement.classList.add('dark');
    });
    await page.screenshot({ path: join(shots, 'whats-new-dark.png') });
    await page.evaluate(() => {
      document.documentElement.classList.remove('dark');
    });
  }

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
