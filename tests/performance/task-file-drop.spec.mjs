import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const dropFile = (page, type, selector, name = 'note.txt') =>
  page.evaluate(
    ({ type, selector, name }) => {
      const transfer = new DataTransfer();
      transfer.items.add(new File(['hello'], name, { type: 'text/plain' }));
      document.querySelector(selector).dispatchEvent(
        new DragEvent(type, {
          bubbles: true,
          cancelable: true,
          dataTransfer: transfer,
        }),
      );
    },
    { type, selector, name },
  );

test('a file dropped on the new task form is attached once the task is created', async ({
  page,
}) => {
  await mockApp(page);
  const attached = [];
  const puts = [];
  await page.route('**/api/workspaces/*/feature-toggles', (route) =>
    route.fulfill({
      json: {
        data: { board: { enabled: true }, 'task.files': { enabled: true } },
      },
    }),
  );
  await page.route('**/api/tasks', (route) =>
    route.request().method() === 'POST'
      ? route.fulfill({
          json: {
            data: {
              ...route.request().postDataJSON(),
              id: 7,
              workspace_id: 1,
              assignees: [],
              checkpoints: [],
            },
          },
        })
      : route.fallback(),
  );
  await page.route('**/api/files/presign-upload', (route) =>
    route.fulfill({
      json: {
        data: {
          key: 'uploads/1/abc/note.txt',
          upload_url: 'http://storage.test/storage-put',
          method: 'PUT',
          content_type: 'text/plain',
          max_bytes: 1000,
        },
      },
    }),
  );
  await page.route('**/storage-put', (route) => {
    puts.push(route.request().method());
    return route.fulfill({ status: 200, body: '' });
  });
  await page.route('**/api/tasks/7/files', (route) => {
    if (route.request().method() === 'POST') {
      attached.push(route.request().postDataJSON());
      return route.fulfill({
        json: {
          data: { id: 11, name: 'note.txt', mime_type: 'text/plain', size: 5 },
        },
      });
    }
    return route.fulfill({
      json: {
        data: attached.map((file, i) => ({
          id: 11 + i,
          name: file.file_name,
          mime_type: file.mime_type,
          size: file.size_bytes,
        })),
      },
    });
  });

  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  const form = page.locator('.new-form-container');
  await expect(form.getByRole('heading', { name: 'Attachments' })).toBeVisible();

  await dropFile(page, 'dragenter', '.new-form-container input');
  await expect(page.getByText('Drop it here to upload')).toBeVisible();
  await dropFile(page, 'drop', '.new-form-container input');
  await expect(page.getByText('Drop it here to upload')).toBeHidden();
  await expect(
    form.getByText('Will upload when the task is created'),
  ).toBeVisible();
  expect(attached).toHaveLength(0);

  await form.getByPlaceholder('Task name').fill('With a file');
  await form.getByTitle('Create').click();

  await expect.poll(() => attached.length).toBe(1);
  expect(attached[0]).toMatchObject({
    file_name: 'note.txt',
    file_path: 'uploads/1/abc/note.txt',
  });
  expect(puts).toEqual(['PUT']);
});

test('dragging text over the task form does not show the file overlay', async ({
  page,
}) => {
  await mockApp(page);
  await page.route('**/api/workspaces/*/feature-toggles', (route) =>
    route.fulfill({
      json: {
        data: { board: { enabled: true }, 'task.files': { enabled: true } },
      },
    }),
  );
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  await expect(page.getByRole('heading', { name: 'Attachments' })).toBeVisible();
  await page.evaluate(() => {
    const transfer = new DataTransfer();
    transfer.setData('text/plain', 'hello');
    document
      .querySelector('.new-form-container input')
      .dispatchEvent(
        new DragEvent('dragenter', {
          bubbles: true,
          cancelable: true,
          dataTransfer: transfer,
        }),
      );
  });
  await expect(page.getByText('Drop it here to upload')).toBeHidden();
});

test('a file dropped while queued files are still uploading is attached too', async ({
  page,
}) => {
  await mockApp(page);
  const attached = [];
  let releasePresign;
  const presignGate = new Promise((resolve) => {
    releasePresign = resolve;
  });
  await page.route('**/api/workspaces/*/feature-toggles', (route) =>
    route.fulfill({
      json: {
        data: { board: { enabled: true }, 'task.files': { enabled: true } },
      },
    }),
  );
  await page.route('**/api/tasks', (route) =>
    route.request().method() === 'POST'
      ? route.fulfill({
          json: {
            data: {
              ...route.request().postDataJSON(),
              id: 7,
              workspace_id: 1,
              assignees: [],
              checkpoints: [],
            },
          },
        })
      : route.fallback(),
  );
  await page.route('**/api/files/presign-upload', async (route) => {
    await presignGate;
    const name = route.request().postDataJSON().file_name;
    await route.fulfill({
      json: {
        data: {
          key: `uploads/1/abc/${name}`,
          upload_url: 'http://storage.test/storage-put',
          method: 'PUT',
          content_type: 'text/plain',
          max_bytes: 1000,
        },
      },
    });
  });
  await page.route('**/storage-put', (route) =>
    route.fulfill({ status: 200, body: '' }),
  );
  await page.route('**/api/tasks/7/files', (route) => {
    if (route.request().method() === 'POST') {
      attached.push(route.request().postDataJSON().file_name);
      return route.fulfill({
        json: { data: { id: attached.length, name: 'x', size: 5 } },
      });
    }
    return route.fulfill({ json: { data: [] } });
  });

  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  const form = page.locator('.new-form-container');
  await expect(
    form.getByRole('heading', { name: 'Attachments' }),
  ).toBeVisible();
  await dropFile(page, 'drop', '.new-form-container input', 'first.txt');
  await form.getByPlaceholder('Task name').fill('Two files');
  await form.getByTitle('Create').click();

  await expect(page.getByText('Uploading files…')).toBeVisible();
  await dropFile(page, 'drop', '.new-form-container input', 'second.txt');
  releasePresign();

  await expect.poll(() => [...attached].sort()).toEqual([
    'first.txt',
    'second.txt',
  ]);
});

const openNewTaskWithFiles = async (page, { presignGate } = {}) => {
  await mockApp(page);
  const attached = [];
  await page.route('**/api/workspaces/*/feature-toggles', (route) =>
    route.fulfill({
      json: {
        data: { board: { enabled: true }, 'task.files': { enabled: true } },
      },
    }),
  );
  await page.route('**/api/tasks', (route) =>
    route.request().method() === 'POST'
      ? route.fulfill({
          json: {
            data: {
              ...route.request().postDataJSON(),
              id: 7,
              workspace_id: 1,
              assignees: [],
              checkpoints: [],
            },
          },
        })
      : route.fallback(),
  );
  await page.route('**/api/files/presign-upload', async (route) => {
    if (presignGate) await presignGate;
    await route.fulfill({
      json: {
        data: {
          key: 'uploads/1/abc/note.txt',
          upload_url: 'http://storage.test/storage-put',
          method: 'PUT',
          content_type: 'text/plain',
          max_bytes: 1000,
        },
      },
    });
  });
  await page.route('**/storage-put', (route) =>
    route.fulfill({ status: 200, body: '' }),
  );
  await page.route('**/api/tasks/7/files', (route) => {
    if (route.request().method() === 'POST') {
      attached.push(route.request().postDataJSON().file_name);
      return route.fulfill({
        json: { data: { id: attached.length, name: 'x', size: 5 } },
      });
    }
    return route.fulfill({ json: { data: [] } });
  });
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  await expect(
    page
      .locator('.new-form-container')
      .getByRole('heading', { name: 'Attachments' }),
  ).toBeVisible();
  return attached;
};

test('a file dropped on a field that cancels the drop itself still goes to the task', async ({
  page,
}) => {
  await openNewTaskWithFiles(page);
  await page.evaluate(() =>
    document
      .querySelector('.new-form-container input')
      .addEventListener('drop', (event) => event.preventDefault()),
  );
  await dropFile(page, 'drop', '.new-form-container input');
  await expect(
    page.getByText('Will upload when the task is created'),
  ).toBeVisible();
});

test('closing the form while its files upload does not hijack the next new task', async ({
  page,
}) => {
  let releasePresign;
  const presignGate = new Promise((resolve) => {
    releasePresign = resolve;
  });
  const attached = await openNewTaskWithFiles(page, { presignGate });
  const form = page.locator('.new-form-container');
  await dropFile(page, 'drop', '.new-form-container input');
  await form.getByPlaceholder('Task name').fill('First');
  await form.getByTitle('Create').click();
  await expect(page.getByText('Uploading files…')).toBeVisible();

  await page.evaluate(() => {
    const store =
      document.querySelector('#app').__vue_app__.config.globalProperties
        .$store;
    store.commit('closeTaskModal');
  });
  await expect(form).toBeHidden();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  await expect(form.getByPlaceholder('Task name')).toHaveValue('');
  releasePresign();
  await expect.poll(() => attached).toEqual(['note.txt']);
  await page.waitForTimeout(300);
  await expect(form.getByTitle('Create')).toBeVisible();
  await expect(form.getByPlaceholder('Task name')).toHaveValue('');
});
