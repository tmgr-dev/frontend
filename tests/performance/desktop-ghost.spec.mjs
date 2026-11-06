import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

test.use({ viewport: { width: 1280, height: 800 } });

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
  });
});

const openDemo = async (page) => {
  const shell = await desktopPage(
    page,
    { account: false },
    { profile: null, upgrade: {} },
  );
  await page.goto('/');
  await expect(page).toHaveURL(/\/welcome/);
  await page.getByTestId('start-local').click();
  await page.getByLabel('Your name').fill('Ada');
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await page.keyboard.press('Escape');
  await page.getByTestId('help-menu').click();
  await page.getByTestId('watch-demo').click();
  await expect(page).toHaveURL(/\/local-demo-launch-week\//);
  return shell;
};

const cardStatus = (shell) =>
  shell('local_db_select', {
    code: 'demo-launch-week',
    sql: `SELECT s.name AS status, t.start_time AS timer FROM tasks t
          JOIN statuses s ON s.id = t.status_id WHERE t.title = 'Fix login redirect'`,
    params: [],
  });

test('the demo plays itself, Esc fast-forwards to the end and nothing counts as a real move', async ({
  page,
}) => {
  const shell = await openDemo(page);
  const panel = page.getByTestId('ghost-panel');
  await expect(panel).toContainText('Simulated locally');
  await expect(panel).toContainText('Press Esc to stop');
  await expect
    .poll(async () => (await cardStatus(shell))[0]?.status, { timeout: 15000 })
    .toBe('In progress');
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('ghost-replay')).toBeVisible();
  const [card] = await cardStatus(shell);
  expect(card).toEqual({ status: 'Review', timer: 0 });
  const runs = await shell('local_db_select', {
    code: 'demo-launch-week',
    sql: `SELECT agent, status FROM agent_work_runs`,
    params: [],
  });
  expect(runs).toEqual([{ agent: 'demo-agent', status: 'succeeded' }]);
  const meta = await shell('local_db_select', {
    code: 'demo-launch-week',
    sql: `SELECT key FROM meta WHERE key LIKE 'onboarding:board_move:%'`,
    params: [],
  });
  expect(meta).toEqual([]);
  await expect(page.getByText('Review').first()).toBeVisible();
});

test('reduced motion shows the finished board and writes nothing until Replay', async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  const shell = await openDemo(page);
  await expect(page.getByTestId('ghost-replay')).toBeVisible();
  await page.waitForTimeout(3000);
  const [card] = await cardStatus(shell);
  expect(card.status).toBe('Review');
  await page.getByTestId('ghost-replay').click();
  await expect(page.getByTestId('ghost-skip')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('ghost-replay')).toBeVisible();
  expect((await cardStatus(shell))[0].status).toBe('Review');
});
