import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

test.use({ viewport: { width: 1280, height: 800 } });

const POPOVER = '[role="dialog"][aria-labelledby^="tour-"]';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
  });
});

const freshStart = async (page, shellOptions = {}) => {
  const shell = await desktopPage(
    page,
    { account: false },
    { profile: null, upgrade: {}, freshInstall: true, ...shellOptions },
  );
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-demo-launch-week\//);
  return shell;
};

const watchIntro = async (page) => {
  const popover = page.locator(POPOVER);
  await expect(popover).toContainText('A demo agent at work');
  await popover
    .getByRole('button', { name: /next|continue|start/i })
    .first()
    .click();
  await expect(page.getByTestId('ghost-panel')).toContainText(
    'Simulated locally',
  );
};

const cardStatus = (shell) =>
  shell('local_db_select', {
    code: 'demo-launch-week',
    sql: `SELECT s.name AS status, t.start_time AS timer, t.common_time AS time FROM tasks t
          JOIN statuses s ON s.id = t.status_id WHERE t.title = 'Fix login redirect'`,
    params: [],
  });

const taskCount = async (shell, code) =>
  (
    await shell('local_db_select', {
      code,
      sql: `SELECT COUNT(*) AS n FROM tasks`,
      params: [],
    })
  )[0].n;

const stage = (page) =>
  page.evaluate(
    () =>
      JSON.parse(localStorage.getItem('tmgr:onboarding:v1') ?? '{}')
        .ghost_stage,
  );

test('a fresh install skips Welcome, plays the demo, hands over the wheel and creates the real workspace', async ({
  page,
}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const shell = await freshStart(page);
  await expect(page.getByTestId('demo-banner')).toBeVisible();
  await expect(page.getByTestId('demo-sign-in')).toBeVisible();
  await expect(page.getByTestId('try-it-panel')).toHaveCount(0);
  await watchIntro(page);
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('ghost-replay')).toBeVisible();
  await expect.poll(() => stage(page)).toBe('wheel');
  const panel = page.getByTestId('try-it-panel');
  await expect(panel).toBeVisible();
  await expect(page.getByTestId('try-it-move')).toHaveAttribute(
    'data-done',
    'false',
  );
  expect(await taskCount(shell, 'demo-launch-week')).toBe(7);
  expect((await cardStatus(shell))[0]).toMatchObject({ status: 'Review' });

  await page.getByTestId('create-my-workspace').click();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await expect(panel).toHaveCount(0);
  await expect.poll(() => stage(page)).toBe('finished');
  expect(await taskCount(shell, 'my-workspace')).toBe(3);
  expect(await taskCount(shell, 'demo-launch-week')).toBe(7);
  expect(await shell('local_workspaces_list')).toHaveLength(2);
  await expect(page.locator(POPOVER)).toContainText('Your local workspace');

  await page.goto('/local-my-workspace/getting-started');
  await expect(page.getByText('Getting started').first()).toBeVisible();
  await expect(page.getByTestId('checklist-card')).toBeVisible();
  expect(errors).toEqual([]);
});

test('relaunching after the ghost keeps the wheel panel and no tour starts again', async ({
  page,
}) => {
  const shell = await freshStart(page);
  await watchIntro(page);
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('try-it-panel')).toBeVisible();
  await page.evaluate(() =>
    localStorage.removeItem('tmgr:onboarding:local-fresh'),
  );
  await page.reload();
  await expect(page).toHaveURL(/\/local-demo-launch-week\//);
  await expect(page.getByTestId('try-it-panel')).toBeVisible();
  await expect(page.locator(POPOVER)).toHaveCount(0);
  await expect(page.getByTestId('ghost-panel')).toContainText('Demo agent');
  expect(await shell('local_workspaces_list')).toHaveLength(2);
});

test('quitting in the middle of the ghost resets the card and the demo plays again', async ({
  page,
}) => {
  const shell = await freshStart(page);
  await watchIntro(page);
  await expect
    .poll(async () => (await cardStatus(shell))[0]?.status, { timeout: 15000 })
    .toBe('In progress');
  await page.evaluate(() =>
    localStorage.removeItem('tmgr:onboarding:local-fresh'),
  );
  await page.reload();
  await expect(page.getByTestId('ghost-panel')).toContainText(
    'Simulated locally',
  );
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('ghost-replay')).toBeVisible();
  const runs = await shell('local_db_select', {
    code: 'demo-launch-week',
    sql: `SELECT agent, status FROM agent_work_runs`,
    params: [],
  });
  expect(runs).toEqual([{ agent: 'demo-agent', status: 'succeeded' }]);
  expect((await cardStatus(shell))[0]).toMatchObject({
    status: 'Review',
    timer: 0,
  });
});

test('time the user tracks on the demo card survives a relaunch', async ({
  page,
}) => {
  const shell = await freshStart(page);
  await watchIntro(page);
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('try-it-panel')).toBeVisible();
  await shell('local_db_select', {
    code: 'demo-launch-week',
    sql: `UPDATE tasks SET common_time = 75 WHERE title = 'Fix login redirect' RETURNING id`,
    params: [],
  });
  await page.evaluate(() =>
    localStorage.removeItem('tmgr:onboarding:local-fresh'),
  );
  await page.reload();
  await expect(page.getByTestId('try-it-panel')).toBeVisible();
  expect((await cardStatus(shell))[0]).toMatchObject({ time: 75 });
});

test('Create and delete the demo leaves only the real workspace', async ({
  page,
}) => {
  const shell = await freshStart(page);
  await watchIntro(page);
  await page.keyboard.press('Escape');
  await page.getByTestId('create-and-delete-demo').click();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await expect.poll(() => shell('local_workspaces_list')).toHaveLength(1);
  expect(await taskCount(shell, 'my-workspace')).toBe(3);
  await page.reload();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await expect(page.getByTestId('demo-offer')).toHaveCount(0);
});

test('Welcome stays for a device that has seen a cloud account', async ({
  page,
}) => {
  await desktopPage(
    page,
    { account: false },
    { profile: null, upgrade: {}, freshInstall: false },
  );
  await page.goto('/');
  await expect(page).toHaveURL(/\/welcome/);
  await expect(page.getByTestId('start-local')).toBeVisible();
});

test('an existing user who watches the demo from the hub gets no wheel panel, no sign-in button and no checklist', async ({
  page,
}) => {
  await desktopPage(
    page,
    { account: false },
    { profile: null, upgrade: {}, freshInstall: false },
  );
  await page.goto('/');
  await page.getByTestId('start-local').click();
  await page.getByLabel('Your name').fill('Ada');
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await page.keyboard.press('Escape');
  await page.getByTestId('help-menu').click();
  await page.getByTestId('watch-demo').click();
  await expect(page).toHaveURL(/\/local-demo-launch-week\//);
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('ghost-replay')).toBeVisible();
  await expect(page.getByTestId('try-it-panel')).toHaveCount(0);
  await expect(page.getByTestId('demo-sign-in')).toHaveCount(0);
  expect(await stage(page)).toBeUndefined();
  await page.goto('/local-my-workspace/getting-started');
  await expect(page.getByTestId('checklist-card')).toHaveCount(0);
});
