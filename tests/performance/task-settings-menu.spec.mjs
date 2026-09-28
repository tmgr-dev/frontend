import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const mockSettings = async (page) => {
  const saved = [];
  await page.route('**/api/tasks/settings', (route) =>
    route.fulfill({
      json: {
        data: [
          {
            id: 9,
            key: 'task_cost',
            name: 'Cost per hour',
            component_type: 'text',
          },
        ],
      },
    }),
  );
  await page.route('**/api/tasks/*/settings', (route) => {
    if (route.request().method() === 'PUT') {
      saved.push({
        url: new URL(route.request().url()).pathname,
        body: route.request().postDataJSON(),
      });
    }
    return route.fulfill({ json: { data: [] } });
  });
  return saved;
};

const editSettings = async (page) => {
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('Cost per hour')).toBeVisible();
  await dialog.locator('input').first().fill('42');
  await dialog.getByRole('button', { name: 'Save' }).click();
  await expect(dialog).toBeHidden();
};

test('task settings open from the three-dots menu of a list row', async ({
  page,
}) => {
  await mockApp(page);
  const saved = await mockSettings(page);
  await page.goto('/');
  const row = page.locator('[data-task-id="1"]');
  await row.getByRole('button', { name: 'Task actions', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Settings' }).click();

  await editSettings(page);
  expect(saved).toHaveLength(1);
  expect(saved[0].url).toBe('/api/tasks/1/settings');
  await expect(page.locator('.new-form-container')).toHaveCount(0);
});

test('task settings open from the three-dots menu of a board card', async ({
  page,
}) => {
  await mockApp(page);
  const saved = await mockSettings(page);
  await page.goto('/demo/board');
  const card = page.locator('[data-task-id="1"]').first();
  await card.getByRole('button', { name: 'Task actions', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Settings' }).click();

  await editSettings(page);
  expect(saved.map((s) => s.url)).toEqual(['/api/tasks/1/settings']);
});
