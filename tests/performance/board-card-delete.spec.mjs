import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const desktopShell = async (page, { nativeConfirm }) => {
  await page.addInitScript((answer) => {
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      invoke: () => Promise.resolve(null),
      transformCallback: () => 0,
    };
    window.confirm = () => answer;
  }, nativeConfirm);
};

const openDeleteConfirm = async (page) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { taskCount: 3 });
  const deletes = [];
  const nativeDialogs = [];
  page.on('dialog', (dialog) => {
    nativeDialogs.push(dialog.message());
    return dialog.dismiss();
  });
  await page.route(/\/api\/tasks\/\d+$/, (route) => {
    if (route.request().method() !== 'DELETE') return route.fallback();
    deletes.push(route.request().url());
    return route.fulfill({
      json: { data: { deleted_at: '2026-10-04T10:00:00Z' } },
    });
  });
  await page.goto('/demo/board');
  const cardRoot = page.locator('[data-task-id="2"]');
  await cardRoot.waitFor();
  await cardRoot.hover();
  await cardRoot.getByRole('button', { name: 'Task actions' }).click();
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  return { deletes, cardRoot, nativeDialogs };
};

test('board card Delete asks in the app and deletes the card', async ({
  page,
}) => {
  const { deletes, cardRoot, nativeDialogs } = await openDeleteConfirm(page);
  await page.getByRole('button', { name: 'Ok' }).click();
  await expect.poll(() => deletes.length).toBe(1);
  await expect(cardRoot).toHaveCount(0);
  expect(nativeDialogs).toEqual([]);
});

test('board card Delete cancel keeps the card', async ({ page }) => {
  const { deletes, cardRoot } = await openDeleteConfirm(page);
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(cardRoot).toHaveCount(1);
  expect(deletes).toEqual([]);
});

test('board card Delete works in the desktop shell where native confirm says no', async ({
  page,
}) => {
  await desktopShell(page, { nativeConfirm: false });
  const { deletes, cardRoot } = await openDeleteConfirm(page);
  await page.getByRole('button', { name: 'Ok' }).click();
  await expect.poll(() => deletes.length).toBe(1);
  await expect(cardRoot).toHaveCount(0);
});
