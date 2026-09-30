import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

const OPEN_BUTTON = 'Open in a separate window';

test('the desktop button opens the task in its own window and closes the modal', async ({
  page,
}) => {
  const shell = await desktopPage(page);
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const form = page.locator('.new-form-container');
  await expect(form.getByPlaceholder('Task name')).toHaveValue('Original task');

  await form.getByTitle(OPEN_BUTTON).click();

  await expect
    .poll(() => shell.taskWindows)
    .toEqual([{ taskId: 1, workspaceCode: 'demo', title: 'Original task' }]);
  await expect(page.locator('.new-form-container')).toHaveCount(0);
});

test('on the web the button stays a link to the task page', async ({ page }) => {
  await mockApp(page);
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const link = page
    .locator('.new-form-container')
    .getByTitle('Open advanced form');
  await expect(link).toHaveAttribute('href', '/demo/tasks/1');
  await expect(page.getByTitle(OPEN_BUTTON)).toHaveCount(0);
});

test('a task window shows only the task form and names itself after the task', async ({
  page,
}) => {
  const shell = await desktopPage(
    page,
    {},
    { windowLabel: 'task-demo-1', lastSeenVersion: null },
  );
  await page.goto('/demo/task-window/1');
  const form = page.locator('.new-form-container');
  await expect(form.getByPlaceholder('Task name')).toHaveValue('Original task');

  await expect(page.getByTitle(OPEN_BUTTON)).toHaveCount(0);
  await expect(page.getByTitle('Open advanced form')).toHaveCount(0);
  await expect(page.getByTestId('whats-new-modal')).toHaveCount(0);
  await expect(page.locator('[data-name="task-side-panel"]')).toHaveCount(0);
  await expect(page.getByTitle('Switch workspace')).toHaveCount(0);
  await expect.poll(() => shell.titles).toContain('Original task');
  expect(new URL(page.url()).pathname).toBe('/demo/task-window/1');
});
