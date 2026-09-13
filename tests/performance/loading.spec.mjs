import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

test('list retains rows while refreshing and renders updated fields', async ({
  page,
}) => {
  const api = await mockApp(page);
  await page.goto('/');
  await expect(page.locator('[data-task-id="1"]')).toContainText(
    'Original task',
  );
  await page.getByPlaceholder('search task').fill('Updated');
  await expect.poll(api.ready).toBe(true);
  await expect(page.locator('[data-task-id="1"]')).toContainText(
    'Original task',
  );
  await expect(page.getByText('Updating tasks…')).toBeVisible();
  api.release();
  await expect(page.locator('[data-task-id="1"]')).toContainText(
    'Updated task',
  );
  expect(api.requests()).toBe(2);
});

for (const failCategories of [false, true]) {
  test(`list query navigation and history after ${
    failCategories ? 'initial failure and retry' : 'initial success'
  }`, async ({ page }) => {
    const api = await mockApp(page, { failCategories });
    await page.goto('/');
    if (failCategories) {
      await expect(
        page.getByRole('button', { name: 'Retry', exact: true }),
      ).toBeVisible();
      api.recover();
      await page.getByRole('button', { name: 'Retry', exact: true }).click();
    }
    const row = page.locator('[data-task-id="1"]');
    await expect(row).toContainText('Original task');
    await page.evaluate(async () => {
      const { default: router } = await import('/src/router/index.js');
      window.__listRow = document.querySelector('[data-task-id="1"]');
      await router.push({ query: { page: '2' } });
    });
    await expect(row).toContainText('Second page task');
    expect(
      await page.evaluate(
        () => window.__listRow === document.querySelector('[data-task-id="1"]'),
      ),
    ).toBe(true);
    await page.goBack();
    await expect(row).toContainText('Original task');
    await page.goForward();
    await expect(row).toContainText('Second page task');
  });
}

test('selection does not transfer to replacement tasks', async ({ page }) => {
  await mockApp(page, { replaceIdentity: true });
  await page.goto('/');
  await expect(page.locator('[data-task-id="1"]')).toBeVisible();
  await page.getByTitle('Tasks selection mode').click();
  await page.getByRole('button', { name: 'Select all', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Process (1)', exact: true }),
  ).toBeVisible();
  await page.evaluate(async () => {
    const { default: router } = await import('/src/router/index.js');
    await router.push({ query: { page: '2' } });
  });
  await expect(page.locator('[data-task-id="99"]')).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Process (1)', exact: true }),
  ).toHaveCount(0);
});

test('Board loads nullable descriptions and keeps filter navigation usable', async ({
  page,
}) => {
  const api = await mockApp(page);
  await page.goto('/');
  await expect(page.locator('[data-task-id="1"]')).toBeVisible();
  await page.evaluate(async () => {
    const { default: router } = await import('/src/router/index.js');
    await router.push('/demo/board');
  });
  await expect(
    page.getByText('Original task', { exact: true }).first(),
  ).toBeVisible();
  await page.evaluate(async () => {
    const { default: router } = await import('/src/router/index.js');
    await router.push({ query: { search: 'Original' } });
  });
  await expect(
    page.getByText('Original task', { exact: true }).first(),
  ).toBeVisible();
  await expect.poll(api.boardRequests).toBe(2);
  await expect(page.getByText('Could not load board')).toHaveCount(0);
});

test('closing edited task flushes to its own ID before another task opens', async ({
  page,
}) => {
  const api = await mockApp(page);
  await page.goto('/');
  await expect(page.locator('[data-task-id="1"]')).toBeVisible();
  await page.evaluate(async () => {
    const { default: router } = await import('/src/router/index.js');
    await router.push('/demo/tasks/1');
  });
  await expect(page.getByPlaceholder('Task name')).toHaveValue('Original task');
  await page.getByPlaceholder('Task name').fill('Saved before leaving');
  await page.evaluate(async () => {
    const { default: router } = await import('/src/router/index.js');
    await router.push('/demo/tasks/2');
  });
  await expect(page.getByPlaceholder('Task name')).toHaveValue('Other task');
  await expect
    .poll(() =>
      api.savedTasks.some(
        ({ id, payload }) =>
          id === 1 && payload.title === 'Saved before leaving',
      ),
    )
    .toBe(true);
  expect(
    api.savedTasks.some(
      ({ id, payload }) => id === 2 && payload.title === 'Saved before leaving',
    ),
  ).toBe(false);
});

test('viewport cards render a bounded window, scroll and drag between columns', async ({
  page,
}) => {
  await mockApp(page, { taskCount: 200, multiStatus: true });
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$router.push('/demo/board'),
  );
  const column = page.locator('.board-card-draggable[data-status="1"]');
  await expect(column.locator('.task-drag-handle').first()).toBeVisible();
  expect(await column.locator('.task-drag-handle').count()).toBeLessThan(30);
  await column.locator('[data-task-id="200"]').first().scrollIntoViewIfNeeded();
  await expect(
    page.getByText('Fixture task 200', { exact: true }),
  ).toBeVisible();
  await column.locator('[data-task-id="1"]').first().scrollIntoViewIfNeeded();
  await expect(
    column.getByText('Original task', { exact: true }),
  ).toBeVisible();
  const handle = column.locator('[data-task-id="1"] .task-drag-handle').first();
  await expect(handle).toBeVisible();
  await handle.click({ trial: true });
  const box = await handle.boundingBox();
  const target = await page
    .locator('.board-card-draggable[data-status="2"]')
    .boundingBox();

  const statusSaved = page.waitForRequest(
    (request) =>
      request.method() === 'PUT' && request.url().endsWith('/api/tasks/1/2'),
  );
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 15, box.y + 15, { steps: 5 });
  await expect(page.locator('.sortable-fallback')).toBeVisible();
  await page.mouse.move(target.x + target.width / 2, target.y + 35, {
    steps: 20,
  });
  await expect(
    page
      .locator('.board-card-draggable[data-status="2"] [data-task-id="1"]')
      .first(),
  ).toBeVisible();
  await page.mouse.up();
  await statusSaved;
  await expect(
    page
      .locator('.board-card-draggable[data-status="2"] [data-task-id="1"]')
      .first(),
  ).toBeVisible();
});

test('immediate editor close serializes input before EditorJS delayed change event', async ({
  page,
}) => {
  const api = await mockApp(page, { blockTask: true });
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$router.push('/demo/tasks/1'),
  );
  const paragraph = page
    .locator('.ce-paragraph[contenteditable="true"]')
    .first();
  await expect(paragraph).toBeVisible();
  await paragraph.fill('Block edited just before closing');
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$router.push('/demo/list'),
  );
  await expect
    .poll(() =>
      api.savedTasks.some(
        ({ id, payload }) =>
          id === 1 &&
          payload.description_json?.blocks?.some(
            (block) => block.data.text === 'Block edited just before closing',
          ),
      ),
    )
    .toBe(true);
});

test('restoring a block draft saves the restored document, not the previous editor DOM', async ({
  page,
}) => {
  const api = await mockApp(page, { blockTask: true });
  await page.addInitScript(() =>
    sessionStorage.setItem(
      'tmgr:task-draft:1:1:1',
      JSON.stringify({
        id: 1,
        workspace_id: 1,
        title: 'Recovered title',
        description: null,
        description_json: {
          version: '2.30.7',
          blocks: [{ type: 'paragraph', data: { text: 'Recovered block' } }],
        },
      }),
    ),
  );
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$router.push('/demo/tasks/1'),
  );
  await expect(page.locator('.ce-paragraph').first()).toHaveText(
    'Initial block',
  );
  await page.getByRole('button', { name: 'Restore draft' }).click();
  await expect(page.locator('.ce-paragraph').first()).toHaveText(
    'Recovered block',
  );
  await expect
    .poll(() =>
      api.savedTasks.some(({ payload }) =>
        payload.description_json?.blocks?.some(
          (block) => block.data.text === 'Recovered block',
        ),
      ),
    )
    .toBe(true);
  expect(
    api.savedTasks.some(
      ({ payload }) =>
        payload.title === 'Recovered title' &&
        payload.description_json?.blocks?.some(
          (block) => block.data.text === 'Initial block',
        ),
    ),
  ).toBe(false);
});

test('route changes do not refetch active timers', async ({ page }) => {
  await mockApp(page);
  const reads = [];
  page.on('request', (request) => {
    if (request.method() === 'GET' && /tasks\/runned/.test(request.url()))
      reads.push(request.url());
  });
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  const initial = reads.length;
  expect(initial).toBeGreaterThan(0);
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$router.push('/demo/board'),
  );
  await expect(
    page
      .locator('.board-card-draggable')
      .getByText('Original task', { exact: true }),
  ).toBeVisible();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$router.push('/demo/list'),
  );
  await page.locator('[data-task-id="1"]').waitFor();
  expect(reads.length).toBe(initial);
});
