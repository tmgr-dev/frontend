import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const convertRoutine = async (page, title) => {
  await desktopPage(page);
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);

  await page.goto('/routines');
  await page.getByPlaceholder(/Quick add/).fill(title);
  await page.keyboard.press('Enter');
  await page.getByText(title, { exact: true }).first().click();
  await page.getByText('Move to a workspace as a regular task…').click();
  const workspace = page.locator('select').filter({ hasText: 'Personal' });
  await workspace.selectOption({ label: 'Personal' });
  await page.getByRole('button', { name: 'Convert', exact: true }).click();
};

test('after converting a routine the user can open the new task right away', async ({
  page,
}) => {
  await convertRoutine(page, 'Pay rent');

  const dialog = page.getByRole('alertdialog');
  await expect(dialog).toContainText('Open the task now?');
  await dialog.getByRole('button', { name: 'Yes' }).click();

  await expect(dialog).toBeHidden();
  await expect(
    page.locator('.new-form-container').getByPlaceholder('Task name'),
  ).toHaveValue('Pay rent');
});

test('declining leaves the routines page without opening the task', async ({
  page,
}) => {
  await convertRoutine(page, 'Call mom');

  const dialog = page.getByRole('alertdialog');
  await dialog.getByRole('button', { name: 'No' }).click();

  await expect(dialog).toBeHidden();
  await expect(page.locator('.new-form-container')).toHaveCount(0);
  await expect(page.getByPlaceholder("What's the routine?")).toHaveCount(0);
});
