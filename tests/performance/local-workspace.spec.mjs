import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

/** The desktop shell's local-workspace commands, run on node:sqlite (Node ≥ 22) instead of Rust. */
const fakeShell = async () => {
  const { DatabaseSync } = await import('node:sqlite');
  const workspaces = [];
  const dbs = new Map();
  const db = (code) => {
    if (!dbs.has(code)) dbs.set(code, new DatabaseSync(':memory:'));
    return dbs.get(code);
  };
  return async (command, args = {}) => {
    switch (command) {
      case 'local_workspaces_list':
        return workspaces;
      case 'local_workspace_create': {
        const code = args.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
        const workspace = {
          id: -1000 - workspaces.length,
          name: args.name,
          code,
          schema_version: 0,
          created_at: new Date().toISOString(),
          path: `/tmp/${code}`,
          database: `/tmp/${code}/workspace.db`,
        };
        workspaces.push(workspace);
        return workspace;
      }
      case 'local_db_select':
        return db(args.code).prepare(args.sql).all(...args.params);
      case 'local_db_execute': {
        const result = db(args.code).prepare(args.sql).run(...args.params);
        return {
          rowsAffected: Number(result.changes),
          lastInsertId: Number(result.lastInsertRowid),
        };
      }
      default:
        return null;
    }
  };
};

test('a local workspace is created from the switcher and keeps its tasks off the server', async ({
  page,
}) => {
  const shell = await fakeShell();
  await page.exposeFunction('__shellInvoke', (command, args) => shell(command, args));
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      invoke: (command, args) => window.__shellInvoke(command, args ?? {}),
      convertFileSrc: (path, protocol) => `${location.origin}/__${protocol}/${encodeURIComponent(path)}`,
      transformCallback: () => 0,
    };
  });
  await mockApp(page);
  const stored = new Map();
  await page.route('**/__tmgrfile/**', async (route) => {
    const key = decodeURIComponent(new URL(route.request().url()).pathname.replace('/__tmgrfile/', ''));
    const method = route.request().method();
    if (method === 'PUT') {
      stored.set(key, route.request().postDataBuffer());
      await route.fulfill({ status: 200, body: '' });
    } else if (method === 'DELETE') {
      stored.delete(key);
      await route.fulfill({ status: 204, body: '' });
    } else if (stored.has(key)) {
      await route.fulfill({ status: 200, contentType: 'image/png', body: stored.get(key) });
    } else {
      await route.fulfill({ status: 404, body: '' });
    }
  });
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
  expect([...stored.keys()].some((key) => key.startsWith('personal/') && key.endsWith('/notes.png'))).toBe(true);
  await page.keyboard.press('Escape');
  await page.goto('/local-personal/board');
  await expect(page.getByText('Written offline').first()).toBeVisible();

  const workspaceCalls = sent.filter(
    (path) =>
      !/^(user|user\/settings|v2\/user\/settings|user\/feature-toggles|workspaces|tasks\/runned|notifications(\/.*)?|broadcasting\/auth)$/.test(
        path,
      ),
  );
  expect(workspaceCalls).toEqual([]);
  expect(settingsPayloads.join(' ')).not.toMatch(/"value":-\d/);
});
