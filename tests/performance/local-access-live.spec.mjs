import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const PERMISSIONS = [
  'tasks:read',
  'tasks:write',
  'statuses:read',
  'comments:read',
  'comments:write',
  'relations:read',
  'relations:write',
  'agent_work:read',
  'agent_work:write',
];

test('persona writes over the local socket update the open board and task panel live', async ({
  page,
}) => {
  const shell = await desktopPage(page);
  let requestId = 0;
  const ask = async (method, path, body) => {
    const id = ++requestId;
    await page.evaluate(
      (payload) => window.__emit('local-access://request', payload),
      {
        id,
        workspaceCode: 'personal',
        workspaceId: -1000,
        personaUuid: 'p-1',
        personaName: 'Companion',
        tokenId: 'local-token-1',
        method,
        path,
        body: body === undefined ? null : JSON.stringify(body),
      },
    );
    await expect.poll(() => shell.accessReplies.some((r) => r.id === id)).toBe(true);
    const reply = shell.accessReplies.find((r) => r.id === id);
    expect(reply.status, reply.body).toBeLessThan(300);
    return reply.body ? JSON.parse(reply.body) : null;
  };
  const mcp = (name, args) =>
    ask('POST', '/mcp', {
      jsonrpc: '2.0',
      id: 1,
      method: 'tools/call',
      params: { name, arguments: args },
    });

  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);

  const now = new Date().toISOString();
  await shell('local_db_execute', {
    code: 'personal',
    sql: `INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
          VALUES ('p-1', 1, 'Test User', 'Companion', NULL, NULL, ?, NULL)`,
    params: [now],
  });
  await shell('local_db_execute', {
    code: 'personal',
    sql: `INSERT INTO workspace_personas (persona_uuid, permissions, enabled_at, disabled_at) VALUES ('p-1', ?, ?, NULL)`,
    params: [JSON.stringify(PERMISSIONS), now],
  });
  await expect
    .poll(() => page.evaluate(() => (window.__listeners['local-access://request'] ?? []).length))
    .toBeGreaterThan(0);

  const statuses = (await ask('GET', '/api/workspaces/statuses')).data;
  const [backlog, next] = statuses;
  const task = (await ask('POST', '/api/tasks', { title: 'Watched task', status_id: backlog.id })).data;
  const other = (await ask('POST', '/api/tasks', { title: 'Other task', status_id: backlog.id })).data;
  const comment = (await ask('POST', `/api/tasks/${task.id}/comments`, { message: 'First note' })).data;

  await page.goto('/local-personal/board');
  const column = (status) => page.locator(`.board-card-draggable[data-column-id='${status.id}']`);
  await expect(column(backlog)).toContainText('Watched task');

  await ask('POST', '/api/tasks', { title: 'Created by persona', status_id: backlog.id });
  await expect(column(backlog)).toContainText('Created by persona');

  await ask('PATCH', `/api/tasks/${task.id}`, { status_id: next.id });
  await expect(column(next)).toContainText('Watched task');
  await expect(column(backlog)).not.toContainText('Watched task');

  await column(next).getByText('Watched task').first().click();
  const panel = page.getByRole('dialog').last();
  await expect(panel.getByText('First note')).toBeVisible();

  await ask('POST', `/api/tasks/${task.id}/comments`, { message: 'Reply from the companion' });
  await expect(panel.getByText('Reply from the companion')).toBeVisible();

  await mcp('add_comment', { taskId: task.id, message: 'Reply over MCP' });
  await expect(panel.getByText('Reply over MCP')).toBeVisible();

  await ask('POST', `/api/comments/${comment.id}/reactions/toggle`, { emoji: '🚀' });
  await expect(panel.getByRole('button', { name: /🚀\s*1/ })).toBeVisible();

  await expect(panel.getByText('Other task')).toHaveCount(0);
  await ask('POST', `/api/tasks/${task.id}/related-to/${other.id}/with/3`);
  await expect(panel.getByText('Other task')).toBeVisible();

  await ask('PATCH', `/api/tasks/${task.id}`, { title: 'Renamed by persona', description: 'Persona text' });
  await expect(panel.getByText('This task was updated by another user')).toBeVisible();
  await panel.getByRole('button', { name: 'Apply Changes' }).click();
  await expect(panel.locator('textarea').first()).toHaveValue('Renamed by persona');
  await expect(column(next)).toContainText('Renamed by persona');

  const run = (await ask('POST', `/api/tasks/${task.id}/agent-work`, { agent: 'claude-code', model: 'companion-model' })).data;
  await expect(panel.getByText('companion-model')).toBeVisible();
  await expect(panel.getByText('running')).toBeVisible();
  await mcp('finish_agent_work', { runId: run.id, status: 'succeeded', summary: 'Done by the companion' });
  await expect(panel.getByText('Done by the companion')).toBeVisible();

  await panel.getByPlaceholder('Write a comment…').first().fill('Typed in the panel');
  await panel.getByPlaceholder('Write a comment…').first().press('Enter');
  await expect(panel.getByText('Typed in the panel')).toHaveCount(1);
  await page.waitForTimeout(500);
  await expect(panel.getByText('Typed in the panel')).toHaveCount(1);
  await expect(panel.getByText('Reply from the companion')).toHaveCount(1);
});
