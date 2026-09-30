import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const openTask = async (page) => {
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const form = page.locator('.new-form-container');
  await expect(form.getByPlaceholder('Task name')).toHaveValue('Original task');
  return form;
};

const withAiReplySetting = async (page, value) => {
  const user = {
    id: 1,
    name: 'Test User',
    email: 'test@example.test',
    settings: [
      { id: 5, key: 'current_workspace', value: '1' },
      { id: 20, key: 'comment_ai_reply', value },
    ],
  };
  const saved = [];
  const asked = [];
  const posted = [];
  await page.route('**/api/user', (route) =>
    route.fulfill({ json: { data: user } }),
  );
  await page.route('**/api/v2/user/settings', async (route) => {
    const payload = route.request().postDataJSON();
    saved.push(payload);
    const next = payload.find((item) => item.id === 20)?.value;
    user.settings = user.settings.map((s) =>
      s.key === 'comment_ai_reply' ? { ...s, value: next } : s,
    );
    await route.fulfill({ json: { data: user } });
  });
  await page.route('**/api/tasks/1/comments/help', async (route) => {
    asked.push(route.request().postDataJSON());
    await route.fulfill({ json: { data: {} } });
  });
  await page.route('**/api/tasks/1/comments', async (route) => {
    if (route.request().method() === 'POST')
      posted.push(route.request().postDataJSON());
    await route.fulfill({ json: { data: [] } });
  });
  return { saved, asked, posted };
};

test('the modal footer keeps every action in one compact row', async ({
  page,
}) => {
  await mockApp(page);
  const form = await openTask(page);
  const footer = form.locator('footer');
  for (const name of ['Open advanced form', 'Save', 'Delete'])
    await expect(footer.getByLabel(name)).toBeVisible();
  const row = footer.getByLabel('Save').locator('xpath=../..');
  await expect(row.getByLabel('Open advanced form')).toBeVisible();
  expect(
    (await footer.getByLabel('Save').boundingBox()).height,
  ).toBeLessThanOrEqual(32);
  expect((await footer.boundingBox()).height).toBeLessThan(96);
});

test('the footer fits a phone screen without horizontal scroll', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockApp(page);
  const form = await openTask(page);
  await expect(form.locator('footer').getByLabel('Delete')).toBeInViewport();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    390,
  );
});

test('AI reply is a remembered toggle that routes Enter to the AI', async ({
  page,
}) => {
  await mockApp(page);
  const calls = await withAiReplySetting(page, '0');
  const form = await openTask(page);
  const toggle = form.getByRole('button', { name: 'AI reply: off' });
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');

  await toggle.click();
  await expect(
    form.getByRole('button', { name: 'AI reply: on' }),
  ).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(() => calls.saved.length).toBe(1);
  expect(calls.saved[0]).toContainEqual({ id: 20, value: '1' });

  const input = form.getByPlaceholder('Write a comment…');
  await input.fill('Why is this slow?');
  await input.press('Enter');
  await expect
    .poll(() => calls.asked)
    .toEqual([{ question: 'Why is this slow?' }]);
  expect(calls.posted).toEqual([]);
});

test('a stored AI reply setting is applied when a task opens', async ({
  page,
}) => {
  await mockApp(page);
  const calls = await withAiReplySetting(page, '1');
  const form = await openTask(page);
  await expect(
    form.getByRole('button', { name: 'AI reply: on' }),
  ).toBeVisible();

  await form.getByRole('button', { name: 'AI reply: on' }).click();
  await expect(
    form.getByRole('button', { name: 'AI reply: off' }),
  ).toBeVisible();
  const input = form.getByPlaceholder('Write a comment…');
  await input.fill('Plain note');
  await input.press('Enter');
  await expect.poll(() => calls.posted).toEqual([{ message: 'Plain note' }]);
  expect(calls.asked).toEqual([]);
});
