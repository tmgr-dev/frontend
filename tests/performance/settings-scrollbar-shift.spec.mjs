import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

// Playwright hides scrollbars by default, which hides this bug: with classic
// scrollbars the centered settings page jumped sideways whenever one tab was
// tall enough to scroll and the next one was not.
test.use({
  viewport: { width: 1728, height: 1000 },
  launchOptions: { ignoreDefaultArgs: ['--hide-scrollbars'] },
});

const navLeft = (page) =>
  page
    .locator('nav[aria-label="Settings"]')
    .evaluate((nav) => Math.round(nav.getBoundingClientRect().left));

test('switching between a tall and a short settings tab keeps the page in place', async ({
  page,
}) => {
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
  });
  await mockApp(page);
  await page.goto('/settings?tab=theme');
  const heading = page.locator('[data-page-header] h1').first();
  await expect(heading).toHaveText('Theme');
  const settingsNav = page.locator('nav[aria-label="Settings"]');

  const tall = await page.evaluate(
    () =>
      document.documentElement.scrollHeight >
      document.documentElement.clientHeight,
  );
  expect(tall).toBe(true);
  const before = await navLeft(page);

  await settingsNav.getByRole('link', { name: 'About', exact: true }).click();
  await expect(heading).toHaveText('About');
  const short = await page.evaluate(
    () =>
      document.documentElement.scrollHeight <=
      document.documentElement.clientHeight,
  );
  expect(short).toBe(true);

  expect(await navLeft(page)).toBe(before);
});
