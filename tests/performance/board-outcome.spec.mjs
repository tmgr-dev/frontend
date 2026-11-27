import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

async function confirmArchive(page, card) {
  await card.getByRole('button', { name: 'Task actions' }).click();
  await page.getByRole('menuitem', { name: 'Archive' }).click();
  await page.getByRole('button', { name: 'Ok', exact: true }).click();
}

test('archive survives a realtime removal; retry only sends the note', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 740 });
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page, { taskCount: 3 });
  await page.route('**/api/user/settings', (r) =>
    r.fulfill({ json: { data: { settings: { askForBoardOutcome: true } } } }),
  );
  const statuses = [
    { id: 1, name: 'Backlog', type: 'default' },
    { id: 9, name: 'Archive', type: 'archived' },
  ].map((s, i) => ({
    ...s,
    color: '#888888',
    pivot: { order: i + 1, is_active: true },
  }));
  await page.route(/\/api\/(workspaces\/)?statuses$/, (r) =>
    r.fulfill({ json: { data: statuses } }),
  );
  let complete;
  let archives = 0;
  await page.route('**/api/tasks/2/9', async (r) => {
    archives++;
    await new Promise((resolve) => {
      complete = resolve;
    });
    await r.fulfill({ json: { data: { id: 2, status_id: 9 } } });
  });
  const notes = [];
  await page.route(/\/api\/tasks\/2\/comments\?workspace_id=1$/, (r) => {
    notes.push(r.request().postDataJSON().message);
    return r.fulfill({
      status: notes.length === 1 ? 422 : 200,
      json: { data: { id: 10 } },
    });
  });
  await page.goto('/demo/board');
  const card = page.locator('[data-task-id="2"]');
  await confirmArchive(page, card);
  await expect.poll(() => archives).toBe(1);
  await confirmArchive(page, card);
  await page.evaluate(async () => {
    const { default: store } = await import('/src/store/index.js');
    store.commit('updateSingleTask', {
      id: 2,
      status_id: 9,
      title: 'Fixture task 2',
    });
  });
  await expect(card).toHaveCount(0);
  await expect(page.getByRole('dialog', { name: /Closing note/ })).toHaveCount(
    0,
  );
  complete();
  const dialog = page.getByRole('dialog', { name: /Fixture task 2/ });
  await dialog.getByRole('button', { name: 'Duplicate' }).click();
  await expect(dialog.getByRole('textbox')).toHaveValue('Duplicate');
  await dialog.getByRole('textbox').fill('  Follow-up elsewhere  ');
  await dialog.getByRole('button', { name: 'Post comment' }).click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  await dialog.getByRole('button', { name: 'Post comment' }).click();
  await expect(dialog).toHaveCount(0);
  expect(notes).toEqual(['Follow-up elsewhere', 'Follow-up elsewhere']);
  expect(archives).toBe(1);
  await expect(page.locator('.board-container[tabindex="-1"]')).toBeFocused();
});

test('closing a local card keeps its note in its workspace database', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 740 });
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:onboarding:demo-offered', 'declined');
    localStorage.setItem(
      'tmgr:onboarding:v1',
      JSON.stringify({ intent: 'skipped', tips_mode: 'off' }),
    );
    localStorage.setItem(
      'local.account.preferences',
      JSON.stringify({ askForBoardOutcome: true }),
    );
  });
  const requests = [];
  const shell = await desktopPage(
    page,
    { account: false, apiRequests: requests },
    {
      profile: {
        id: -1,
        uuid: '00000000-0000-4000-8000-000000000001',
        name: 'Ada',
        created_at: '2026-10-11T10:00:00Z',
      },
    },
  );
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await page.goto('/local-my-workspace/board');
  await page
    .getByRole('button', { name: 'New task', exact: true })
    .first()
    .click();
  await page.getByPlaceholder('Task name').fill('Local outcome fixture');
  await page.keyboard.press('ControlOrMeta+KeyS');
  const rows = () =>
    shell('local_db_select', {
      code: 'my-workspace',
      sql: 'SELECT id FROM tasks WHERE title=?',
      params: ['Local outcome fixture'],
    });
  await expect.poll(async () => (await rows()).length).toBe(1);
  const [{ id }] = await rows();
  await page.goto('/local-my-workspace/board');
  await confirmArchive(page, page.locator(`[data-task-id="${id}"]`));
  const dialog = page.getByRole('dialog', { name: /Local outcome fixture/ });
  await dialog.getByRole('textbox').fill('Private closing note');
  await dialog.getByRole('button', { name: 'Post comment' }).click();
  await expect(dialog).toHaveCount(0);
  const comments = await shell('local_db_select', {
    code: 'my-workspace',
    sql: 'SELECT message FROM comments WHERE task_id=?',
    params: [id],
  });
  expect(comments).toEqual([{ message: 'Private closing note' }]);
  expect(JSON.stringify(requests)).not.toMatch(
    /Private closing note|\/comments/,
  );
});

for (const action of ['menu', 'drag']) {
  test(`a successful ${action} close posts or skips an outcome and restores card focus`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.addInitScript(() =>
      sessionStorage.setItem('tmgr:tours:auto', 'true'),
    );
    await mockApp(page, { taskCount: 3, multiStatus: true });
    await page.route('**/api/user/settings', (route) =>
      route.fulfill({
        json: { data: { settings: { askForBoardOutcome: true } } },
      }),
    );
    const statuses = [
      { id: 1, name: 'Working', type: 'active' },
      { id: 2, name: 'Accepted', type: 'completed' },
    ].map((s, i) => ({
      ...s,
      color: '#888888',
      pivot: { order: i + 1, is_active: true },
    }));
    await page.route(/\/api\/(workspaces\/)?statuses$/, (route) =>
      route.fulfill({ json: { data: statuses } }),
    );
    const posts = [];
    page.on('request', (req) => {
      if (req.method() === 'POST' && req.url().includes('/comments'))
        posts.push(req.url());
    });
    await page.goto('/demo/board');
    const card = page.locator('[data-task-id="1"]').first();
    if (action === 'menu') {
      await card.getByRole('button', { name: 'Task actions' }).click();
      await page.getByTestId('task-move-to-status').hover();
      await page.getByTestId('task-move-to-status-2').click();
    } else {
      const handle = card.locator('.task-drag-handle');
      await handle.waitFor();
      const box = await handle.boundingBox();
      const target = await page
        .locator('.board-card-draggable[data-status="2"]')
        .boundingBox();
      await page.mouse.move(box.x + 5, box.y + 5);
      await page.mouse.down();
      await page.mouse.move(box.x + 17, box.y + 17, { steps: 5 });
      await page.mouse.move(target.x + target.width / 2, target.y + 35, {
        steps: 20,
      });
      await page.mouse.up();
    }
    const dialog = page.getByRole('dialog', { name: /Closing note/ });
    await expect(dialog).toBeVisible();
    if (action === 'menu') {
      await dialog.getByRole('button', { name: 'Done', exact: true }).click();
      await dialog.getByRole('button', { name: 'Post comment' }).click();
    } else await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(
      page.locator(
        '.board-card-draggable[data-status="2"] [data-task-id="1"] [data-card-menu]',
      ),
    ).toBeFocused();
    expect(posts).toHaveLength(action === 'menu' ? 1 : 0);
    if (action === 'menu') {
      await expect(
        page.locator('[data-task-id="1"] [data-testid="task-comment-count"]'),
      ).toContainText('1');
    }
  });
}
