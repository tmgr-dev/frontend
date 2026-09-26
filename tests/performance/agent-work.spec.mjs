import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

test('task panel shows agent work apart from the human time', async ({
  page,
}) => {
  await mockApp(page);
  const startedAt = new Date(Date.now() - 5 * 60 * 1000).toISOString();
  await page.route('**/api/tasks/1/agent-work', (route) =>
    route.fulfill({
      json: {
        data: {
          runs: [
            {
              id: 2,
              task_id: 1,
              workspace_id: 1,
              user_id: 1,
              agent: 'claude-code',
              model: 'claude-opus-5-5',
              session_id: null,
              branch: 'feat/tm-253-agent-runs',
              status: 'running',
              started_at: startedAt,
              ended_at: null,
              duration_seconds: 300,
              summary: 'Adding the agent work section',
              pr_url: null,
              commits: [],
              tests: null,
            },
            {
              id: 1,
              task_id: 1,
              workspace_id: 1,
              user_id: 1,
              agent: 'codex',
              model: null,
              session_id: null,
              branch: 'feat/tm-253-agent-runs',
              status: 'succeeded',
              started_at: '2026-09-26T08:00:00Z',
              ended_at: '2026-09-26T09:20:00Z',
              duration_seconds: 4800,
              summary: 'Backend: agent_work_runs table, REST and MCP tools',
              pr_url: 'https://github.com/tmgr-dev/backend/pull/148',
              commits: [
                { sha: 'e7134ec0', message: 'feat(agent-work): track AI agent work' },
              ],
              tests: { passed: 84, failed: 0, command: 'mvn test' },
            },
          ],
          totals: { agent_seconds: 5100, human_seconds: 900 },
        },
      },
    }),
  );

  await page.goto('/');
  await page.locator('[data-task-id="1"]').first().click();

  const section = page.locator('section', { hasText: 'Agent work' });
  await expect(section).toBeVisible();
  await expect(section.getByText('Claude Code')).toBeVisible();
  await expect(section.getByText('Codex')).toBeVisible();
  await expect(section.getByText(/Agents 1h 2\dm · You 15m 0s/)).toBeVisible();
  await expect(section.getByText('Pull request')).toHaveAttribute(
    'href',
    'https://github.com/tmgr-dev/backend/pull/148',
  );
  await section.getByText('1 commit').click();
  await expect(section.getByText('e7134ec')).toBeVisible();

  if (process.env.AGENT_WORK_SHOT)
    await section.screenshot({ path: process.env.AGENT_WORK_SHOT });
});
