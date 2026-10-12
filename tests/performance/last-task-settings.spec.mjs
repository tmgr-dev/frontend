import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const PROMPT = 'Apply settings from the previous task?';
const KEY = 'tmgr:last-task-settings:demo';

const setup = async (page, seed = {}) => {
  await page.addInitScript((seed) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('preferred_editor', 'markdown');
    for (const [key, value] of Object.entries(seed))
      localStorage.setItem(key, JSON.stringify(value));
  }, seed);
  await mockApp(page, { multiStatus: true });
  await page.route('**/api/project_categories*', (route) =>
    route.fulfill({
      json: {
        data: [
          { id: 5, title: 'Alpha', code: 'ALP', settings: [], workspace_id: 1 },
        ],
      },
    }),
  );
  const posts = [];
  await page.route('**/api/tasks', (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    const body = route.request().postDataJSON();
    posts.push(body);
    return route.fulfill({
      json: {
        data: {
          ...body,
          id: 6 + posts.length,
          workspace_id: 1,
          assignees: [],
          checkpoints: [],
        },
      },
    });
  });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().waitFor();
  return { posts };
};

const commit = (page, mutation, payload) =>
  page.evaluate(
    ({ mutation, payload }) =>
      document
        .querySelector('#app')
        .__vue_app__.config.globalProperties.$store.commit(mutation, payload),
    { mutation, payload },
  );

const openNewForm = async (page) => {
  await commit(page, 'setShowCreatingTaskModal', 1);
  const form = page.locator('.new-form-container');
  await expect(form.getByTitle('Create')).toBeVisible();
  await expect(form.locator('[data-tour="task-form.status"]')).toHaveText(
    /Backlog/,
  );
  return form;
};

const closeForm = async (page) => {
  await commit(page, 'closeTaskModal');
  await expect(page.locator('.new-form-container')).toHaveCount(0);
};

const categoryButton = (form) =>
  form.locator('[data-tour="task-form.category"] button[role="combobox"]');

test('without saved settings the new task form shows no prompt', async ({
  page,
}) => {
  await setup(page);
  const form = await openNewForm(page);
  await expect(form.getByText(PROMPT)).toHaveCount(0);
});

test('a created task remembers its settings; Apply fills them but not the title or description', async ({
  page,
}) => {
  const { posts } = await setup(page);
  let form = await openNewForm(page);
  await expect(form.getByText(PROMPT)).toHaveCount(0);

  await form.locator('[data-tour="task-form.status"]').click();
  await page.getByRole('option', { name: 'In progress' }).click();
  await categoryButton(form).click();
  await page.getByRole('option', { name: 'Alpha' }).click();
  await expect(categoryButton(form)).toHaveText(/Alpha/);
  await form.getByTitle('Assign to me').click();
  await form.getByPlaceholder('Task name').fill('First task');
  await form.locator('.cm-content').click();
  await page.keyboard.type('First description');
  await form.getByTitle('Create').click();

  await expect.poll(() => posts.length).toBe(1);
  expect(posts[0]).toMatchObject({
    status_id: 2,
    project_category_id: 5,
    assignees: [1],
  });
  const readSaved = () =>
    page.evaluate((key) => JSON.parse(localStorage.getItem(key)), KEY);
  await expect.poll(readSaved).not.toBeNull();
  const saved = await readSaved();
  expect(saved).toMatchObject({
    status_id: 2,
    project_category_id: 5,
    assignees: [1],
  });
  expect(saved).not.toHaveProperty('title');
  expect(saved).not.toHaveProperty('description');
  expect(JSON.stringify(saved)).not.toContain('First');

  await closeForm(page);
  form = await openNewForm(page);
  await expect(form.getByText(PROMPT)).toBeVisible();
  await expect(categoryButton(form)).toHaveText(/Category/);
  await expect(form.getByTitle('Assign to me')).toBeVisible();

  await form.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(form.getByText(PROMPT)).toHaveCount(0);
  await expect(form.locator('[data-tour="task-form.status"]')).toHaveText(
    /In progress/,
  );
  await expect(categoryButton(form)).toHaveText(/Alpha/);
  await expect(form.getByTitle('Assign to me')).toHaveCount(0);
  await expect(form.getByPlaceholder('Task name')).not.toHaveValue(
    'First task',
  );
  await expect(form.locator('.cm-content')).not.toContainText(
    'First description',
  );

  await form.getByPlaceholder('Task name').fill('Second task');
  await form.getByTitle('Create').click();
  await expect.poll(() => posts.length).toBe(2);
  expect(posts[1]).toMatchObject({
    title: 'Second task',
    status_id: 2,
    project_category_id: 5,
    assignees: [1],
  });
  expect(posts[1].description ?? '').not.toContain('First description');
});

test('Apply carries a remembered estimate into the created task', async ({
  page,
}) => {
  const { posts } = await setup(page, {
    [KEY]: {
      status_id: 2,
      project_category_id: null,
      assignees: [],
      persona_assignees: [],
      approximately_time: 5400,
    },
  });
  const form = await openNewForm(page);
  await form.getByRole('button', { name: 'Apply', exact: true }).click();
  await form.getByPlaceholder('Task name').fill('Estimated task');
  await form.getByTitle('Create').click();
  await expect.poll(() => posts.length).toBe(1);
  expect(posts[0]).toMatchObject({ status_id: 2, approximately_time: 5400 });
});

test('Dismiss hides the prompt without applying; the settings stay offered on the next form', async ({
  page,
}) => {
  await setup(page, {
    [KEY]: {
      status_id: 2,
      project_category_id: 5,
      assignees: [1],
      persona_assignees: [],
      approximately_time: 0,
    },
  });
  let form = await openNewForm(page);
  await expect(form.getByText(PROMPT)).toBeVisible();
  await form.getByRole('button', { name: 'Dismiss', exact: true }).click();
  await expect(form.getByText(PROMPT)).toHaveCount(0);
  await expect(form.locator('[data-tour="task-form.status"]')).toHaveText(
    /Backlog/,
  );
  await expect(categoryButton(form)).toHaveText(/Category/);
  await expect(form.getByTitle('Assign to me')).toBeVisible();

  await closeForm(page);
  form = await openNewForm(page);
  await expect(form.getByText(PROMPT)).toBeVisible();
});

test('settings saved for another workspace are not offered here', async ({
  page,
}) => {
  await setup(page, {
    'tmgr:last-task-settings:other': {
      status_id: 2,
      project_category_id: 5,
      assignees: [1],
      persona_assignees: [],
      approximately_time: 3600,
    },
  });
  const form = await openNewForm(page);
  await expect(form.getByText(PROMPT)).toHaveCount(0);
});
