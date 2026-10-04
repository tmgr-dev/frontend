import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const shots = process.env.TASK_MENU_SHOTS;

const shot = async (page, name) => {
  if (!shots) return;
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${shots}/${name}.png` });
};

const plugin = (id, name, titles) => ({
  folder: id,
  manifest: JSON.stringify({
    id,
    name,
    version: '1.0.0',
    engines: { tmgr: '^1.6' },
    permissions: ['menus:task', 'tasks:read', 'notifications'],
    contributes: {
      commands: titles.map((_, i) => ({ id: `${id}.c${i}`, title: `C${i}` })),
      menus: {
        'task/card': titles.map((title, i) => ({
          command: `${id}.c${i}`,
          title,
        })),
      },
    },
  }),
  code: titles
    .map(
      (_, i) => `
    tmgr.commands.register('${id}.c${i}', async (args) => {
      await tmgr.ui.notify('${id} c${i} ' + JSON.stringify(args.taskId));
    });`,
    )
    .join('\n'),
});

const sender = plugin('dev.sender', 'Sender', ['Send to chat', 'Copy link']);
const extra = plugin('dev.extra', 'Extra', ['Extra one', 'Extra two', 'Extra three']);

const setup = async (page, devPlugins, enable) => {
  await page.addInitScript(() =>
    localStorage.setItem('plugins.devMode', 'true'),
  );
  const shell = await desktopPage(page, {}, { devPlugins });
  await page.setViewportSize({ width: 1100, height: 760 });
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  const [backlog] = await shell('local_db_select', {
    code: 'personal',
    sql: `SELECT id FROM statuses WHERE type = 'default'`,
    params: [],
  });
  for (const title of ['First task', 'Second task'])
    await shell('local_db_execute', {
      code: 'personal',
      sql: `INSERT INTO tasks (title, status_id, created_at, updated_at) VALUES (?, ?, '', '')`,
      params: [title, backlog.id],
    });
  await page.locator('[data-sidebar="footer"] button').first().click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();
  for (const name of enable)
    await page
      .locator('article', { hasText: name })
      .getByRole('switch')
      .click();
};

const items = (page) => page.getByTestId('plugin-task-menu-item');

test('plugin items show in the board card, list and task page menus and run with the task id', async ({
  page,
}) => {
  await setup(page, [sender], ['Sender']);

  await page.goto('/local-personal/board');
  const card = page.locator('[data-task-id]', { hasText: 'Second task' }).first();
  await card.getByRole('button', { name: 'Task actions' }).click();
  await expect(items(page)).toHaveText([/Send to chat/, /Copy link/]);
  await expect(page.getByTestId('plugin-task-menu-separator')).toHaveCount(1);
  await expect(page.getByTestId('plugin-task-menu-submenu')).toHaveCount(0);
  const menuItems = page.getByRole('menuitem');
  const labels = await menuItems.allTextContents();
  expect(labels.findIndex((t) => /Move to bottom/.test(t))).toBeLessThan(
    labels.findIndex((t) => /Send to chat/.test(t)),
  );
  expect(labels.findIndex((t) => /Send to chat/.test(t))).toBeLessThan(
    labels.findIndex((t) => /Archive/.test(t)),
  );
  await shot(page, 'board-card-menu');
  await page.keyboard.press('Escape');
  await page.evaluate(() => document.documentElement.classList.add('dark'));
  await card.getByRole('button', { name: 'Task actions' }).click();
  await expect(items(page).first()).toBeVisible();
  await shot(page, 'board-card-menu-dark');
  await page.keyboard.press('Escape');
  await page.evaluate(() => document.documentElement.classList.remove('dark'));

  await card.getByRole('button', { name: 'Task actions' }).click();
  await items(page).first().click();
  await expect(page.getByText('dev.sender c0 2').first()).toBeVisible();

  await page.goto('/local-personal/list');
  const row = page.locator('[data-task-id]', { hasText: 'First task' }).first();
  await row.locator('button[aria-label="Task actions"]').click();
  await expect(items(page)).toHaveCount(2);
  await shot(page, 'list-menu');
  await items(page).nth(1).click();
  await expect(page.getByText('dev.sender c1 1').first()).toBeVisible();

  await page.goto('/local-personal/tasks/2');
  await expect(page.getByPlaceholder('Task name')).toHaveValue('Second task');
  await page.getByRole('button', { name: 'Task actions' }).click();
  await expect(items(page)).toHaveCount(2);
  await expect(page.getByTestId('plugin-task-menu-separator')).toHaveCount(0);
  await shot(page, 'task-page-menu');
  await items(page).first().click();
  await expect(page.getByText('dev.sender c0 2').first()).toBeVisible();
});

test('more than four items collapse into a Plugins submenu', async ({ page }) => {
  await setup(page, [sender, extra], ['Sender', 'Extra']);
  await page.goto('/local-personal/board');
  const card = page.locator('[data-task-id]', { hasText: 'First task' }).first();
  await card.getByRole('button', { name: 'Task actions' }).click();
  const trigger = page.getByTestId('plugin-task-menu-submenu');
  await expect(trigger).toBeVisible();
  await expect(items(page)).toHaveCount(0);
  await trigger.click();
  await expect(items(page)).toHaveCount(5);
  await shot(page, 'submenu');
  await items(page).nth(3).click();
  await expect(page.getByText('dev.extra c1 1').first()).toBeVisible();
});

test('a stopped plugin and a user without plugins see no plugin group', async ({
  page,
}) => {
  await setup(page, [sender], []);
  await page.goto('/local-personal/board');
  const card = page.locator('[data-task-id]', { hasText: 'First task' }).first();
  await card.getByRole('button', { name: 'Task actions' }).click();
  await expect(page.getByRole('menuitem', { name: /Archive/ })).toBeVisible();
  await expect(items(page)).toHaveCount(0);
  await expect(page.getByTestId('plugin-task-menu-separator')).toHaveCount(0);
  await page.keyboard.press('Escape');
  await page.goto('/local-personal/tasks/1');
  await expect(page.getByPlaceholder('Task name')).toHaveValue('First task');
  await expect(page.getByRole('button', { name: 'Task actions' })).toHaveCount(0);
});

test('the detached task window shows the items the main window sends and relays a run', async ({
  page,
}) => {
  await desktopPage(page, {}, { windowLabel: 'task-demo-1', lastSeenVersion: null });
  await page.goto('/demo/task-window/1');
  await expect(page.getByPlaceholder('Task name')).toHaveValue('Original task');
  await expect(page.getByRole('button', { name: 'Task actions' })).toHaveCount(0);
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window.__emitted ?? []).filter((e) => e.event === 'plugin-task-menu://request'),
      ),
    )
    .toEqual([
      expect.objectContaining({
        target: { kind: 'AnyLabel', label: 'main' },
        payload: { label: 'task-demo-1' },
      }),
    ]);

  await page.evaluate(() =>
    window.__emit('plugin-task-menu://items', {
      items: [
        { pluginId: 'dev.sender', pluginName: 'Sender', command: 'dev.sender.c0', title: 'Send to chat' },
      ],
    }),
  );
  await page.getByRole('button', { name: 'Task actions' }).click();
  await expect(items(page)).toHaveText([/Send to chat/]);
  await shot(page, 'detached-window-menu');
  await items(page).first().click();

  const findRun = () =>
    page.evaluate(() =>
      window.__emitted.find((e) => e.event === 'plugin-task-menu://run'),
    );
  await expect.poll(findRun).toBeTruthy();
  const run = await findRun();
  expect(run.target).toEqual({ kind: 'AnyLabel', label: 'main' });
  expect(run.payload).toMatchObject({
    label: 'task-demo-1',
    pluginId: 'dev.sender',
    command: 'dev.sender.c0',
    taskId: 1,
  });
  await page.evaluate(
    (requestId) =>
      window.__emit('plugin-task-menu://result', { requestId, error: 'The plugin is busy' }),
    run.payload.requestId,
  );
  await expect(page.getByText('The plugin command failed', { exact: true })).toBeVisible();
  await expect(page.getByText('The plugin is busy', { exact: true })).toBeVisible();
});
