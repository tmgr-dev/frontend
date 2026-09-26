import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

test('a local workspace is created from the switcher and keeps its tasks off the server', async ({
  page,
}) => {
  const shell = await desktopPage(page);
  const stored = shell.files;
  const sent = [];
  const settingsPayloads = [];
  page.on('request', (request) => {
    const url = new URL(request.url());
    if (!url.pathname.startsWith('/api/')) return;
    const path = url.pathname.replace('/api/', '');
    sent.push(path);
    if (path === 'v2/user/settings') settingsPayloads.push(request.postData() ?? '');
  });

  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();

  await expect(page).toHaveURL(/\/local-personal\//);
  sent.length = 0;
  await page.goto('/local-personal/board');
  await expect(page.getByText('Backlog').first()).toBeVisible();
  await expect(page.getByText('In progress').first()).toBeVisible();
  await expect(page.getByRole('button', { name: 'Invite user' })).toHaveCount(0);
  await expect(page.getByTestId('local-workspace-badge').first()).toBeVisible();

  await page.getByRole('button', { name: 'Open Add Task Modal' }).first().click();
  await page.getByPlaceholder('Task name').fill('Written offline');
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(async () => (await shell('local_db_select', {
    code: 'personal',
    sql: 'SELECT title FROM tasks',
    params: [],
  })).map((row) => row.title)).toContain('Written offline');
  const attachments = page.locator('.task-attachments').first();
  await expect(attachments).toBeVisible();
  await attachments.locator('input[type="file"]').setInputFiles({
    name: 'notes.png',
    mimeType: 'image/png',
    buffer: Buffer.from(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aKzsAAAAASUVORK5CYII=',
      'base64',
    ),
  });
  await expect(page.getByText('notes.png').first()).toBeVisible();
  await expect
    .poll(() => [...stored.entries()].find(([key]) => key.startsWith('personal/') && key.endsWith('/notes.png'))?.[1].length)
    .toBeGreaterThan(0);
  await attachments.scrollIntoViewIfNeeded();
  await expect
    .poll(() => attachments.locator('img').first().evaluate((img) => img.complete && img.naturalWidth))
    .toBeGreaterThan(0);
  await attachments.locator('input[type="file"]').setInputFiles({
    name: 'framework_blank.stl',
    mimeType: '',
    buffer: Buffer.from('solid blank\nendsolid blank\n'),
  });
  await expect(page.getByText('framework_blank.stl').first()).toBeVisible();
  await expect(page.getByText('This file type cannot be attached.')).toHaveCount(0);
  await page.evaluate(() =>
    window.__emit('download://finished', {
      name: 'notes.png',
      path: '/home/user/Downloads/notes.png',
      success: true,
    }),
  );
  await expect(page.getByText('Downloaded notes.png')).toBeVisible();
  await page.getByRole('button', { name: 'Show in Finder' }).click();
  await expect.poll(() => shell.revealed).toEqual(['/home/user/Downloads/notes.png']);
  await page.keyboard.press('Escape');
  await page.goto('/local-personal/board');
  await expect(page.getByText('Written offline').first()).toBeVisible();

  await shell('local_db_execute', {
    code: 'personal',
    sql: `INSERT INTO categories (title, code, settings, created_at, updated_at) VALUES ('Parent', 'PA', '[]', '', ''), ('Nested', NULL, '[]', '', '')`,
    params: [],
  });
  await shell('local_db_execute', { code: 'personal', sql: `UPDATE categories SET parent_id = 1 WHERE id = 2`, params: [] });
  await shell('local_db_execute', { code: 'personal', sql: `UPDATE tasks SET project_category_id = 1`, params: [] });
  await page.goto('/local-personal/categories/1/children');
  await expect(page.getByText('Nested').first()).toBeVisible();
  await expect(page.getByText('Written offline').first()).toBeVisible();
  await expect(page.getByText(/Not available in local workspaces/)).toHaveCount(0);
  await page.getByRole('button', { name: 'Create subcategory' }).click();
  await expect(page).toHaveURL(/\/local-personal\/categories\/1\/create$/);
  await expect(page.getByText('Parent category').locator('..')).toContainText('Parent');
  await expect(page.getByText('Parent category').locator('..')).not.toContainText('Select');
  await page.getByPlaceholder('Name').fill('Fresh child');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page).toHaveURL(/\/local-personal\/categories\/3\/children$/);
  expect(
    (await shell('local_db_select', { code: 'personal', sql: 'SELECT parent_id FROM categories WHERE id = 3', params: [] }))[0]
      .parent_id,
  ).toBe(1);

  await page.goto('/settings/workspaces');
  await expect(page.getByRole('button', { name: 'Save' })).toBeVisible();
  await expect(page.getByText('Workspace Invitations')).toHaveCount(0);
  await expect(page.getByText('Workspace Members')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Invite|Delete workspace|Exit/ })).toHaveCount(0);
  await expect(page.getByText(/Not available in local workspaces/)).toHaveCount(0);

  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /Export to Markdown/ }).click();
  await expect.poll(() => shell.exports.length).toBe(1);
  const exported = shell.exports[0].files.map((file) => file.path);
  expect(exported[0]).toBe('README.md');
  expect(exported.some((path) => /^tasks\/T-\d+-written-offline\.md$/.test(path))).toBe(true);

  const workspaceCalls = sent.filter(
    (path) =>
      !/^(user|user\/settings|v2\/user\/settings|user\/feature-toggles|workspaces|tasks\/runned|notifications(\/.*)?|broadcasting\/auth)$/.test(
        path,
      ),
  );
  expect(workspaceCalls).toEqual([]);
  expect(settingsPayloads.join(' ')).not.toMatch(/"value":-\d/);
});
