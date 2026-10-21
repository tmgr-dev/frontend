import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const nowIso = (minutesAgo) =>
  new Date(Date.now() - minutesAgo * 60 * 1000).toISOString();

const session = (id, taskId, state, extra = {}) => ({
  session_id: id,
  workspace_id: 1,
  task: { id: taskId, key: `TM-${taskId}` },
  state,
  state_since: nowIso(5),
  started_at: nowIso(30),
  updated_at: nowIso(1),
  ended_at: null,
  agent_profile: 'claude-code',
  host_label: 'work-laptop',
  user: { id: 1, name: 'Test User' },
  stale: false,
  ...extra,
});

const sessions = [
  session('sess-working', 1, 'working'),
  session('sess-blocked', 2, 'blocked', { agent_profile: '<b>x</b>' }),
  session('sess-idle-a', 2, 'idle'),
  session('sess-done', 3, 'done', { stale: true }),
];

const stubStatus = async (page, status = 200) => {
  await page.route('**/api/workspaces/*/agent-status', (route) =>
    status === 200
      ? route.fulfill({ json: { data: sessions } })
      : route.fulfill({ status, json: { error: 'feature_disabled' } }),
  );
};

const shot = async (page, name) => {
  if (process.env.AGENT_STATUS_SHOTS)
    await page.screenshot({
      path: `${process.env.AGENT_STATUS_SHOTS}/${name}.png`,
    });
};

test('board cards show the agent state chip', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { taskCount: 3 });
  await stubStatus(page);
  await page.goto('/demo/board');

  const chip = (id) =>
    page.locator(`[data-task-id="${id}"] [data-testid="agent-status-chip"]`);
  await expect(chip(1)).toHaveAttribute('data-state', 'working');
  await expect(chip(2)).toHaveAttribute('data-state', 'blocked');
  await expect(chip(2)).toContainText('2');
  await expect(chip(3)).toHaveAttribute('data-state', 'done');
  await expect(chip(2)).toHaveAttribute(
    'title',
    /Blocked: <b>x<\/b> on work-laptop/,
  );
  await expect(chip(2).locator('b')).toHaveCount(0);
  await shot(page, 'board');
});

test('chip slot keeps the card height whether or not a session exists', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { taskCount: 3 });
  await page.route('**/api/workspaces/*/agent-status', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 600));
    await route.fulfill({ json: { data: sessions } });
  });
  await page.goto('/demo/board');
  const card = page.locator('[data-task-id="1"]');
  await card.waitFor();
  const before = (await card.boundingBox()).height;
  await expect(card.locator('[data-testid="agent-status-chip"]')).toBeVisible();
  expect((await card.boundingBox()).height).toBe(before);
});

test('nothing is shown when agent work is off in the workspace', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { taskCount: 3 });
  await stubStatus(page, 403);
  await page.goto('/demo/board');
  await page.locator('[data-task-id="1"]').waitFor();
  await expect(page.locator('[data-testid="agent-status-chip"]')).toHaveCount(
    0,
  );
});

test('list rows and the task panel show the chip', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { taskCount: 3 });
  await stubStatus(page);
  await page.goto('/demo/list');
  await expect(
    page
      .locator('[data-testid="agent-status-chip"][data-state="working"]')
      .first(),
  ).toBeVisible();
  await shot(page, 'list');

  await page.locator('[data-task-id="1"]').first().click();
  await expect(
    page.locator('[data-testid="agent-status-chip"][data-state="working"]'),
  ).toHaveCount(2);
  await page.waitForTimeout(800);
  await shot(page, 'panel');
});

const titleGap = async (page) => {
  const header = await page.locator('.new-form-container header').boundingBox();
  const title = await page
    .locator('[data-tour="task-form.title"]')
    .boundingBox();
  return title.y - (header.y + header.height);
};

const openPanel = async (page, agentStatus) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { taskCount: 3 });
  await page.route('**/api/workspaces/*/agent-status', agentStatus);
  const statusLoaded = page.waitForResponse('**/api/workspaces/*/agent-status');
  await page.goto('/demo/list');
  await statusLoaded;
  await page.locator('[data-task-id="1"]').first().click();
  await page.locator('[data-tour="task-form.title"]').waitFor();
};

test('task panel keeps no empty row above the title without a session', async ({
  page,
}) => {
  await openPanel(page, (route) => route.fulfill({ json: { data: [] } }));
  expect(await titleGap(page)).toBeLessThanOrEqual(24);
  await expect(
    page.locator('.new-form-container [data-testid="agent-status-slot"]'),
  ).toHaveCount(0);
});

test('task panel shows the chip above the title when a session exists', async ({
  page,
}) => {
  await openPanel(page, (route) => route.fulfill({ json: { data: sessions } }));
  const chip = page.locator(
    '.new-form-container [data-testid="agent-status-chip"]',
  );
  await expect(chip).toHaveAttribute('data-state', 'working');
  const chipBox = await chip.boundingBox();
  const title = await page
    .locator('[data-tour="task-form.title"]')
    .boundingBox();
  expect(chipBox.y + chipBox.height).toBeLessThanOrEqual(title.y);
});

test('task panel has no chip row when agent status is refused', async ({
  page,
}) => {
  await openPanel(page, (route) =>
    route.fulfill({ status: 403, json: { error: 'feature_disabled' } }),
  );
  expect(await titleGap(page)).toBeLessThanOrEqual(24);
  await expect(
    page.locator('.new-form-container [data-testid="agent-status-slot"]'),
  ).toHaveCount(0);
});

test('task page shows the chip when a session arrives after it opened', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockApp(page, { taskCount: 3 });
  let release;
  const sessionReady = new Promise((resolve) => {
    release = resolve;
  });
  await page.route('**/api/workspaces/*/agent-status', async (route) => {
    await sessionReady;
    await route.fulfill({ json: { data: sessions } });
  });
  await page.goto('/demo/tasks/1');
  await page.locator('[data-tour="task-form.title"]').waitFor();
  expect(await titleGap(page)).toBeLessThanOrEqual(24);
  release();
  await expect(
    page.locator('.new-form-container [data-testid="agent-status-chip"]'),
  ).toHaveAttribute('data-state', 'working');
});
