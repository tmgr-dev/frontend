import { expect, test } from '@playwright/test';
import {
  countLifecycle,
  lifecycleOf,
  resetLifecycle,
  survivors,
  trackNodes,
} from './incremental-render.helpers.mjs';
import { mockApp } from './mockApp.mjs';

const message = (id, role, content, status = 'done') => ({
  id,
  role,
  content,
  status,
  steps: [],
  created_at: '2026-09-13T10:00:00Z',
});

test('the workspace chat keeps existing messages mounted and untouched', async ({
  page,
}) => {
  await countLifecycle(page);
  await mockApp(page);
  const history = [message(1, 'user', 'hello'), message(2, 'assistant', 'hi')];
  await page.route('**/api/agent/conversations**', async (route) => {
    const url = route.request().url();
    const post = route.request().method() === 'POST';
    if (url.endsWith('/messages') && post)
      return route.fulfill({
        json: { data: { message_id: 3, pending_message_id: 4 } },
      });
    if (url.endsWith('/messages'))
      return route.fulfill({ json: { data: history } });
    return route.fulfill({
      json: {
        data: { id: 7, workspace_id: 1, task_id: null, created_at: 'x' },
      },
    });
  });

  await page.goto('/demo/list');
  await page.getByLabel('Ask AI').click();
  const rows = '[aria-label="AI assistant"] .space-y-3 > *';
  await expect(page.locator(rows)).toHaveCount(2);
  await trackNodes(page, rows);
  await resetLifecycle(page);

  await page.evaluate(() => {
    const store =
      document.querySelector('#app').__vue_app__.config.globalProperties.$store;
    store.commit('setAiPanelOpen', false);
    setTimeout(() => store.commit('setAiPanelOpen', true), 100);
  });
  await page.waitForTimeout(600);
  expect((await lifecycleOf(page, 'AgentMessage')).updated).toBe(0);

  const input = page.getByPlaceholder('Ask about this workspace…');
  await input.fill('question');
  await input.press('Enter');
  await expect(page.locator(rows)).toHaveCount(4);

  expect(await survivors(page, rows)).toEqual({
    kept: 2,
    total: 4,
    removed: 0,
  });
  expect(await lifecycleOf(page, 'AgentMessage')).toEqual({
    added: 2,
    updated: 0,
    removed: 0,
  });
});
