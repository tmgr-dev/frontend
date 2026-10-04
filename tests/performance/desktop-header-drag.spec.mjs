import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const stubTauri = (page) =>
  page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      invoke: () => Promise.resolve(null),
      transformCallback: () => 0,
    };
  });

const INTERACTIVE = 'button, a, input, select, textarea, [role="button"]';

const scanHeader = (page) =>
  page.evaluate((interactive) => {
    const header = document.querySelector('header[data-tauri-drag-region]');
    const rect = header.getBoundingClientRect();
    const y = rect.top + rect.height / 2;
    const dead = [];
    let draggable = 0;
    for (let x = 0; x < rect.width; x += 4) {
      const el = document.elementFromPoint(x, y);
      if (!el || el.closest(interactive)) continue;
      if (el.hasAttribute('data-tauri-drag-region')) draggable++;
      else dead.push(`${Math.round(x)}:${el.tagName}.${String(el.className).slice(0, 40)}`);
    }
    const buttonsMarked = [
      ...header.querySelectorAll('button, a, input'),
    ].filter((el) => el.hasAttribute('data-tauri-drag-region')).length;
    return { dead, draggable, buttonsMarked };
  }, INTERACTIVE);

for (const route of ['board', 'dashboard']) {
  test(`desktop header empty space is a drag region on ${route}`, async ({
    page,
  }) => {
    await stubTauri(page);
    await page.setViewportSize({ width: 1440, height: 800 });
    await mockApp(page, { taskCount: 12 });
    await page.goto(`/demo/${route}`);
    await page.locator('header[data-tauri-drag-region]').waitFor();
    await page.waitForTimeout(1500);
    const { dead, draggable, buttonsMarked } = await scanHeader(page);
    expect(draggable).toBeGreaterThan(0);
    expect(buttonsMarked).toBe(0);
    expect(dead).toEqual([]);
  });
}

test('web header is not marked as drag region target for buttons', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 800 });
  await mockApp(page, { taskCount: 12 });
  await page.goto('/demo/board');
  await page.getByText('Fixture task 2').first().waitFor();
  expect(
    await page.locator('header button[data-tauri-drag-region]').count(),
  ).toBe(0);
});
