import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const mockRoutines = (page) =>
  page.route('**/api/daily-routines/**', (route) =>
    route.fulfill({ json: { data: route.request().url().includes('expand/stats') ? {} : [] } }),
  );

const measure = (page) =>
  page.locator('[data-tour="routines.view-switcher"]').evaluate((el) => {
    const parent = el.parentElement.getBoundingClientRect();
    return {
      width: el.getBoundingClientRect().width,
      parentWidth: parent.width - parseFloat(getComputedStyle(el.parentElement).paddingLeft) - parseFloat(getComputedStyle(el.parentElement).paddingRight),
      tabs: [...el.querySelectorAll('button')].map((b) => b.getBoundingClientRect().width),
      overflow: document.documentElement.scrollWidth - window.innerWidth,
    };
  });

for (const view of ['List', 'Week']) {
  test(`on a phone the view tabs fill the row with equal widths (${view})`, async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await mockApp(page);
    await mockRoutines(page);
    await page.goto('/routines');
    await page.getByTitle(view, { exact: true }).click();
    const m = await measure(page);
    expect(Math.abs(m.width - m.parentWidth)).toBeLessThanOrEqual(1);
    expect(Math.max(...m.tabs) - Math.min(...m.tabs)).toBeLessThanOrEqual(1);
    expect(m.tabs).toHaveLength(5);
    expect(m.overflow).toBeLessThanOrEqual(0);
  });
}

test('on desktop the view tabs keep their inline width', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page);
  await mockRoutines(page);
  await page.goto('/routines');
  const m = await measure(page);
  expect(m.width).toBeLessThan(m.parentWidth / 2);
  expect(Math.max(...m.tabs) - Math.min(...m.tabs)).toBeGreaterThan(1);
});
