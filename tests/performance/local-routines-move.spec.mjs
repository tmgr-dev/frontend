import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const shotsDir = process.env.SHOTS_DIR;

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
  });
});

const shoot = async (page, name) => {
  if (!shotsDir) return;
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${shotsDir}/${name}-light.png` });
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${shotsDir}/${name}-dark.png` });
  await page.emulateMedia({ colorScheme: 'light' });
  await page.waitForTimeout(500);
  const size = page.viewportSize();
  await page.setViewportSize({ width: 360, height: 780 });
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${shotsDir}/${name}-360.png` });
  await page.setViewportSize(size);
};

const cloud = async (
  page,
  { reject = () => null, hold = async () => {} } = {},
) => {
  const sent = [];
  await page.route(
    (url) =>
      /\/api\/(daily-routines\/|tasks$|project_categories)/.test(
        url.pathname + url.search,
      ),
    async (route) => {
      const request = route.request();
      const url = new URL(request.url());
      const path = url.pathname.replace('/api/', '');
      if (path === 'project_categories') {
        await route.fulfill({
          json: { data: [{ id: 12, title: 'Home' }] },
        });
        return;
      }
      if (request.method() !== 'POST') return route.fallback();
      const body = request.postDataJSON();
      const failure = reject(body);
      await hold(body);
      sent.push({ path, body, headers: request.headers() });
      if (failure) {
        await route.fulfill({ status: failure.status, json: failure.json });
        return;
      }
      await route.fulfill({
        status: 201,
        json: { data: { id: 900 + sent.length } },
      });
    },
  );
  return sent;
};

const openLocalRoutines = async (page, cloudOptions) => {
  const shell = await desktopPage(page);
  const sent = await cloud(page, cloudOptions);
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  await page.goto('/routines');
  await expect(page.getByTestId('local-routines-label')).toBeVisible();
  await page.getByPlaceholder(/Quick add/).fill('Water plants');
  await page.keyboard.press('Enter');
  await expect(page.getByText('Water plants').first()).toBeVisible();
  await page.getByRole('button', { name: 'New routine' }).click();
  await page.getByPlaceholder("What's the routine?").fill('Morning stretch');
  await page.getByRole('button', { name: 'Daily', exact: true }).click();
  await page.getByRole('button', { name: 'Create routine' }).click();
  await expect(page.getByText('Morning stretch').first()).toBeVisible();
  const localTitles = async () =>
    (
      await shell('local_db_select', {
        code: 'personal',
        sql: 'SELECT title FROM routines WHERE deleted_at IS NULL ORDER BY title',
        params: [],
      })
    ).map((row) => row.title);
  await expect.poll(localTitles).toEqual(['Morning stretch', 'Water plants']);
  return { localTitles, sent };
};

const selectBoth = async (page) => {
  await page.getByTestId('local-routines-select').click();
  await page.getByLabel('Select all local routines').check();
  await expect(page.getByTestId('move-routines-bar')).toContainText(
    '2 selected',
  );
};

test('selected local routines move to a cloud workspace only after confirmation and leave this device', async ({
  page,
}) => {
  const { localTitles, sent } = await openLocalRoutines(page);
  await selectBoth(page);
  await shoot(page, 'select');

  await page.getByRole('button', { name: 'Move to cloud routines' }).click();
  await page.getByRole('button', { name: 'Continue' }).click();
  const dialog = page.getByRole('dialog');
  await expect(page.getByTestId('move-routines-statement')).toContainText(
    '2 routines will be sent to the TMGR server and will leave this device',
  );
  await expect(dialog).toContainText('Morning stretch');
  await expect(dialog).toContainText('Water plants');
  await expect(dialog).toContainText('Past completions are not moved');
  await shoot(page, 'confirm');
  expect(sent).toEqual([]);
  expect(await localTitles()).toEqual(['Morning stretch', 'Water plants']);

  await page.getByTestId('move-routines-confirm').click();
  await expect(page.getByTestId('move-routines-summary')).toContainText(
    '2 of 2 moved to Demo',
  );
  await shoot(page, 'result');

  expect(sent.map((s) => s.path).sort()).toEqual([
    'daily-routines/tasks',
    'daily-routines/tasks/recurring',
  ]);
  const plain = sent.find((s) => s.path === 'daily-routines/tasks');
  expect(plain.body).toEqual({ title: 'Water plants', workspace_id: 1 });
  const recurring = sent.find((s) => s.path.endsWith('/recurring'));
  expect(recurring.body).toMatchObject({
    title: 'Morning stretch',
    workspace_id: 1,
    recurrence: { frequency: 'DAILY' },
  });
  for (const s of sent) {
    expect(s.headers['x-workspace-id']).toBe('1');
    expect(JSON.stringify(s.body)).not.toMatch(/\b1\d{9}\b/);
  }
  await expect.poll(localTitles).toEqual([]);
});

test('converting to cloud tasks sends the chosen category and the first active status', async ({
  page,
}) => {
  const { localTitles, sent } = await openLocalRoutines(page);
  await selectBoth(page);
  await page.getByRole('button', { name: 'Convert to cloud tasks' }).click();
  await expect(
    page.getByRole('dialog').getByLabel('Category'),
  ).toContainText('Home');
  await page
    .getByRole('dialog')
    .getByLabel('Category')
    .selectOption({ label: 'Home' });
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page.getByRole('dialog')).toContainText(
    'keep the title and description only',
  );
  expect(sent).toEqual([]);
  await page.getByTestId('move-routines-confirm').click();
  await expect(page.getByTestId('move-routines-summary')).toContainText(
    '2 of 2 moved to Demo',
  );

  expect(sent.map((s) => s.path)).toEqual(['tasks', 'tasks']);
  expect(sent.map((s) => s.body).sort((a, b) => a.title.localeCompare(b.title))).toEqual([
    {
      title: 'Morning stretch',
      workspace_id: 1,
      status_id: 1,
      project_category_id: 12,
    },
    {
      title: 'Water plants',
      workspace_id: 1,
      status_id: 1,
      project_category_id: 12,
    },
  ]);
  expect(new Set(sent.map((s) => s.headers['idempotency-key'])).size).toBe(2);
  await expect.poll(localTitles).toEqual([]);
});

test('a rejected routine stays on this device with the server message and moves on retry', async ({
  page,
}) => {
  let reject = true;
  const { localTitles, sent } = await openLocalRoutines(page, {
    reject: (body) =>
      reject && body.title === 'Water plants'
        ? { status: 422, json: { message: 'The title is not allowed.' } }
        : null,
  });
  await selectBoth(page);
  await page.getByRole('button', { name: 'Move to cloud routines' }).click();
  await page.getByRole('button', { name: 'Continue' }).click();
  await page.getByTestId('move-routines-confirm').click();

  await expect(page.getByTestId('move-routines-summary')).toContainText(
    '1 of 2 moved to Demo',
  );
  const failed = page.locator('[data-status="failed"]');
  await expect(failed).toContainText('Water plants');
  await expect(failed).toContainText('The title is not allowed.');
  await shoot(page, 'failed');
  expect(await localTitles()).toEqual(['Water plants']);

  reject = false;
  await page.getByTestId('move-routines-retry').click();
  await expect(page.getByTestId('move-routines-summary')).toContainText(
    '2 of 2 moved to Demo',
  );
  expect(sent.filter((s) => s.body.title === 'Morning stretch')).toHaveLength(1);
  await expect.poll(localTitles).toEqual([]);
});

test('the dialog cannot be closed while a retry is in flight', async ({
  page,
}) => {
  let reject = true;
  let release = () => {};
  let holding = false;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  await openLocalRoutines(page, {
    reject: (body) =>
      reject && body.title === 'Water plants'
        ? { status: 422, json: { message: 'The title is not allowed.' } }
        : null,
    hold: async () => {
      if (holding) await gate;
    },
  });
  await selectBoth(page);
  await page.getByRole('button', { name: 'Move to cloud routines' }).click();
  await page.getByRole('button', { name: 'Continue' }).click();
  await page.getByTestId('move-routines-confirm').click();
  await expect(page.getByTestId('move-routines-summary')).toContainText(
    '1 of 2 moved to Demo',
  );

  reject = false;
  holding = true;
  await page.getByTestId('move-routines-retry').click();
  const done = page.getByTestId('move-routines-done');
  await expect(done).toBeDisabled();
  await expect(page.getByRole('button', { name: /^Open / })).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toBeVisible();
  release();
  await expect(page.getByTestId('move-routines-summary')).toContainText(
    '2 of 2 moved to Demo',
  );
  await expect(done).toBeEnabled();
  await done.click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('a failure to build the server client shows an error and leaves the dialog usable', async ({
  page,
}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.route(/moveToCloud\/cloudHttp\.ts/, (route) =>
    route.fulfill({
      contentType: 'text/javascript',
      body: "export const serverCloudApi = async () => { throw new Error('Client unavailable'); };",
    }),
  );
  const { localTitles } = await openLocalRoutines(page);
  await selectBoth(page);
  await page.getByRole('button', { name: 'Move to cloud routines' }).click();
  await page.getByRole('button', { name: 'Continue' }).click();
  await page.getByTestId('move-routines-confirm').click();
  await expect(page.getByRole('alert')).toContainText('Client unavailable');
  await expect(page.getByTestId('move-routines-confirm')).toBeEnabled();
  expect(errors.filter((m) => /Client unavailable/.test(m))).toEqual([]);
  expect(await localTitles()).toEqual(['Morning stretch', 'Water plants']);
});

test('an account-less desktop offers no move and says why', async ({
  page,
}) => {
  await desktopPage(page, { account: false });
  await page.goto('/');
  await page.getByTestId('start-local').click();
  await page.getByLabel('Your name').fill('Ada');
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await page.goto('/routines');
  await expect(page.getByTestId('local-routines-label')).toBeVisible();
  await expect(page.getByTestId('local-routines-select')).toHaveCount(0);
  await expect(page.getByTestId('local-routines-move-note')).toHaveText(
    'Sign in to move routines to the cloud.',
  );
});
