import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { mockApp } from './mockApp.mjs';

const shotDir = process.env.PERSONA_SHOTS;
if (shotDir) mkdirSync(shotDir, { recursive: true });

const shot = async (page, name, fullPage = false) => {
  if (!shotDir) return;
  for (const scheme of ['light', 'dark']) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.evaluate(
      (dark) => document.documentElement.classList.toggle('dark', dark),
      scheme === 'dark',
    );
    await page.waitForTimeout(500);
    await page.screenshot({
      path: `${shotDir}/${name}-${scheme}.png`,
      fullPage,
    });
  }
};

const me = { id: 1, name: 'Test User', has_avatar: false };
const ann = { id: 2, name: 'Ann Lee', has_avatar: false };

const reviewer = {
  id: 'uuid-reviewer',
  name: 'Reviewer',
  description: 'Reviews incoming tasks',
  avatar_url: null,
  owner: { id: 1, name: 'Test User' },
  workspace_id: null,
};
const triager = {
  id: 'uuid-triager',
  name: 'Triager',
  description: 'Sorts the backlog',
  avatar_url: null,
  owner: { id: 2, name: 'Ann Lee' },
  workspace_id: 1,
};

const withPersonas = async (page, { assigned = [reviewer] } = {}) => {
  await mockApp(page);
  const calls = [];
  let current = {
    assignees: [me, ann],
    persona_assignees: assigned,
  };
  const task = (id = 1) => ({
    id,
    title: 'Original task',
    description: null,
    description_json: null,
    status: 'created',
    status_id: 1,
    workspace_id: 1,
    user_id: 1,
    user: me,
    settings: [],
    checkpoints: [],
    common_time: 0,
    start_time: 0,
    approximately_time: 0,
    project_category_id: null,
    created_at: '2026-09-13T10:00:00Z',
    updated_at: '2026-09-13T10:00:00Z',
    ...current,
  });
  await page.route('**/api/**/feature-toggles', (route) => {
    const toggle = (key) => ({
      key,
      name: key,
      group: 'pages',
      type: 'boolean',
      enabled: true,
    });
    return route.fulfill({
      json: {
        data: route.request().url().includes('/user/')
          ? {
              default_landing_page: {
                key: 'default_landing_page',
                name: 'Default Landing Page',
                group: 'general',
                type: 'select',
                options: ['list', 'board', 'dashboard', 'daily_routines'],
                value: 'list',
              },
              'board.user_filter': toggle('board.user_filter'),
            }
          : { board: toggle('board'), 'task.assignees': toggle('task.assignees') },
      },
    });
  });
  await page.route('**/api/workspaces/1/members*', (route) =>
    route.fulfill({ json: { data: [me, ann] } }),
  );
  await page.route('**/api/workspaces/1/assignable-personas*', (route) =>
    route.fulfill({ json: { data: [reviewer, triager] } }),
  );
  await page.route('**/api/tasks/1', (route) => {
    if (route.request().method() === 'GET')
      return route.fulfill({ json: { data: task() } });
    calls.push({
      method: route.request().method(),
      body: route.request().postDataJSON(),
    });
    return route.fulfill({ json: { data: task() } });
  });
  await page.route('**/api/tasks/1/personas/*', (route) => {
    const uuid = route.request().url().split('/').pop();
    const method = route.request().method();
    calls.push({ method, persona: uuid });
    const found = [reviewer, triager].find((p) => p.id === uuid);
    current = {
      assignees: method === 'POST' ? [me, ann] : current.assignees,
      persona_assignees:
        method === 'POST'
          ? [...current.persona_assignees, found]
          : current.persona_assignees.filter((p) => p.id !== uuid),
    };
    return route.fulfill({ json: { data: task() } });
  });
  await page.route('**/api/tasks/current*', (route) =>
    route.fulfill({
      json: {
        data: [task()],
        meta: {
          current_page: 1,
          per_page: 10,
          total: 1,
          last_page: 1,
          from: 1,
          to: 1,
        },
      },
    }),
  );
  await page.route('**/api/tasks/status/*', (route) =>
    route.fulfill({ json: { data: [task()] } }),
  );
  return { calls };
};

test('a list row shows the persona chip and hides its owner; the chip opens the owner popover', async ({
  page,
}) => {
  await withPersonas(page);
  await page.goto('/demo/list');
  const row = page.locator('[data-task-id="1"]').first();
  await expect(row.getByTestId('persona-chip')).toHaveCount(1);
  await expect(row.getByTestId('persona-chip')).toBeVisible();
  await expect(row.getByText('Ann Lee')).toHaveCount(0);

  await row.getByTestId('persona-chip').click();
  const popover = page.getByTestId('persona-popover');
  await expect(popover).toContainText('Reviewer');
  await expect(popover.locator('xpath=..')).toContainText(
    'persona of Test User',
  );
  await shot(page, 'task-row-persona-chip-popover');
});

test('the board card renders persona chips outside the assignee picker', async ({
  page,
}) => {
  await withPersonas(page);
  await page.goto('/demo/board');
  const chip = page.getByTestId('persona-chip').first();
  await expect(chip).toBeVisible();
  await expect(chip.locator('xpath=ancestor::button[@title="Change assignee"]')).toHaveCount(0);
  await chip.click();
  await expect(page.getByTestId('persona-popover')).toBeVisible();
  await shot(page, 'board-card-persona-chip-popover');
});

test('the task modal picker lists personas and assigns one through the persona endpoint', async ({
  page,
}) => {
  const { calls } = await withPersonas(page);
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const form = page.locator('.new-form-container');
  await expect(form.getByPlaceholder('Task name')).toHaveValue('Original task');
  await expect(form.getByTestId('persona-chip')).toHaveCount(1);

  await form
    .locator('button[role="combobox"]')
    .filter({ hasText: 'Ann Lee, Reviewer' })
    .click();
  await expect(page.getByText('Personas', { exact: true })).toBeVisible();
  await expect(page.getByTestId('persona-option')).toHaveCount(2);
  await shot(page, 'assignee-picker-personas');

  await page.getByTestId('persona-option').filter({ hasText: 'Triager' }).click();
  await expect
    .poll(() => calls.some((c) => c.method === 'POST' && c.persona === 'uuid-triager'))
    .toBe(true);
  await expect(form.getByTestId('persona-chip')).toHaveCount(2);
  await shot(page, 'task-modal-persona-chips');
});

test('saving the task carries persona uuids next to the assignees', async ({
  page,
}) => {
  const { calls } = await withPersonas(page);
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const form = page.locator('.new-form-container');
  const title = form.getByPlaceholder('Task name');
  await expect(title).toHaveValue('Original task');
  await title.fill('Renamed task');
  await expect
    .poll(() => calls.find((c) => c.method === 'PUT')?.body, { timeout: 8000 })
    .toMatchObject({
      assignees: [1, 2],
      persona_assignees: ['uuid-reviewer'],
    });
});

test('Settings -> Personas offers the scope and lists other members workspace personas', async ({
  page,
}) => {
  await mockApp(page);
  const created = [];
  const own = {
    id: 'uuid-reviewer',
    name: 'Reviewer',
    description: 'Reviews incoming tasks',
    avatar_url: null,
    system_prompt: '',
    prompt_version: 1,
    archived_at: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    owner: { id: 1, name: 'Test User' },
    workspace_id: 1,
    scope: 'workspace',
    can_edit: true,
  };
  await page.route('**/api/personas*', async (route) => {
    if (route.request().method() === 'POST') {
      created.push(route.request().postDataJSON());
      return route.fulfill({ json: { data: own } });
    }
    return route.fulfill({
      json: {
        data: [
          own,
          { ...own, id: 'uuid-private', name: 'Scratch', workspace_id: null, scope: 'account' },
        ],
      },
    });
  });
  await page.route('**/api/workspaces/1/personas*', (route) =>
    route.fulfill({
      json: {
        data: [
          {
            persona: {
              id: 'uuid-triager',
              name: 'Triager',
              description: 'Sorts the backlog',
              avatar_url: null,
              archived: false,
              owner: { id: 2, name: 'Ann Lee' },
              workspace_id: 1,
            },
            workspace_id: 1,
            permissions: [],
            blocked: false,
            blocked_at: null,
            effective_permissions: [],
          },
          {
            persona: {
              id: 'uuid-reviewer',
              name: 'Reviewer',
              description: null,
              avatar_url: null,
              archived: false,
              owner: { id: 1, name: 'Test User' },
              workspace_id: 1,
            },
            workspace_id: 1,
            permissions: [],
            blocked: false,
            blocked_at: null,
            effective_permissions: [],
          },
        ],
      },
    }),
  );
  await page.goto('/settings/personas');

  await expect(page.getByLabel('Only me')).toBeChecked();
  await expect(page.getByTestId('persona-scope-badge')).toHaveText([
    'Workspace: Demo',
    'Only me',
  ]);
  const shared = page.getByTestId('workspace-personas');
  await expect(shared).toContainText('Triager');
  await expect(shared).toContainText('persona of Ann Lee');
  await expect(shared).not.toContainText('Reviewer');
  await shot(page, 'settings-personas-scope', true);

  await page.getByLabel(/This workspace/).check();
  await page.getByPlaceholder('Reviewer').fill('Shared bot');
  await page.getByRole('button', { name: 'Create persona' }).click();
  await expect.poll(() => created[0]).toMatchObject({
    name: 'Shared bot',
    workspace_id: 1,
  });
});

test('the board filter narrows to a persona and writes it to the URL', async ({
  page,
}) => {
  await withPersonas(page);
  await page.goto('/demo/board');
  await expect(page.getByTestId('persona-chip').first()).toBeVisible();
  await page.getByRole('button', { name: /Filters/ }).click();
  await page.getByTestId('persona-filter').selectOption('uuid-triager');
  await expect(page).toHaveURL(/persona=uuid-triager/);
  await expect(page.getByTestId('persona-chip')).toHaveCount(0);
  await page.getByTestId('persona-filter').selectOption('mine');
  await expect(page).toHaveURL(/my_personas=1/);
  await expect(page.getByTestId('persona-chip').first()).toBeVisible();
});
