import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

test('the built-in estimate plugin draws its badge, status bar item, page and task section', async ({
  page,
}) => {
  const shell = await desktopPage(page);
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);

  const [inProgress] = await shell('local_db_select', {
    code: 'personal',
    sql: `SELECT id FROM statuses WHERE type = 'active'`,
    params: [],
  });
  await shell('local_db_execute', {
    code: 'personal',
    sql: `INSERT INTO tasks (title, status_id, common_time, approximately_time, created_at, updated_at)
          VALUES ('Over budget', ?, 10800, 7200, '2026-09-26T10:00:00Z', '2026-09-26T10:00:00Z')`,
    params: [inProgress.id],
  });
  await page.goto('/local-personal/board');

  await expect(page.getByText('150%').first()).toBeVisible();
  await expect(
    page.locator('footer').getByText('1h 0m over estimate'),
  ).toBeVisible();

  await page.getByRole('link', { name: 'Estimate vs actual' }).click();
  await expect(page).toHaveURL(
    /\/local-personal\/plugins\/tmgr\.estimate\/report$/,
  );
  await expect(
    page.getByRole('heading', { name: 'Estimate vs actual' }),
  ).toBeVisible();
  await expect(page.getByRole('cell', { name: /Over budget/ })).toBeVisible();

  await page.getByRole('button', { name: /Over budget/ }).click();
  await expect(page.getByText('Spent').first()).toBeVisible();
  await expect(page.getByText('3h 0m').first()).toBeVisible();
  await page.keyboard.press('Escape');

  await page.goto('/settings/plugins');
  const card = page.locator('article', { hasText: 'Estimate vs actual' });
  await expect(card.getByText('Running')).toBeVisible();
  await expect(
    card.getByText('read tasks, read statuses, read tracked time'),
  ).toBeVisible();
  await card.getByRole('switch').click();
  await expect(card.getByText('Off')).toBeVisible();
  await expect(page.locator('footer').getByText('over estimate')).toHaveCount(
    0,
  );
});

const hostile = {
  folder: 'dev.hostile',
  manifest: JSON.stringify({
    id: 'dev.hostile',
    name: 'Hostile',
    version: '1.0.0',
    engines: { tmgr: '^1.0' },
    permissions: ['tasks:read'],
    contributes: {
      commands: [
        { id: 'dev.hostile.spin', title: 'Spin forever' },
        { id: 'dev.hostile.write', title: 'Rename a task' },
      ],
    },
  }),
  code: `
    tmgr.commands.register('dev.hostile.spin', () => { while (true) {} });
    tmgr.commands.register('dev.hostile.write', () => tmgr.tasks.update(1, { title: 'hacked' }));
  `,
};

test('a hostile folder plugin is refused writes, cut off when it spins and turned off after three faults', async ({
  page,
}) => {
  await page.addInitScript(() =>
    localStorage.setItem('plugins.devMode', 'true'),
  );
  const shell = await desktopPage(page, {}, { devPlugins: [hostile] });
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  await shell('local_db_execute', {
    code: 'personal',
    sql: `INSERT INTO tasks (title, created_at, updated_at) VALUES ('Mine', '', '')`,
    params: [],
  });

  await page
    .getByRole('button', { name: 'Yurij' })
    .or(page.locator('[data-sidebar="footer"] button'))
    .first()
    .click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();
  const card = page.locator('article', { hasText: 'Hostile' });
  await expect(card.getByText('Off')).toBeVisible();
  await expect(card.getByText('read tasks')).toBeVisible();
  await card.getByRole('switch').click();
  await expect(card.getByText('Running')).toBeVisible();

  await card.getByRole('button', { name: 'Rename a task' }).click();
  await expect(
    page.getByText('PERMISSION_DENIED: tasks.update needs tasks:write', {
      exact: true,
    }),
  ).toBeVisible();
  const [task] = await shell('local_db_select', {
    code: 'personal',
    sql: 'SELECT title FROM tasks',
    params: [],
  });
  expect(task.title).toBe('Mine');

  for (let i = 0; i < 3; i++) {
    const started = Date.now();
    await card.getByRole('button', { name: 'Spin forever' }).click();
    const responsive = await page.evaluate(() => performance.now());
    expect(responsive).toBeGreaterThan(0);
    await expect(
      page.getByText('The plugin command failed').first(),
    ).toBeVisible();
    expect(Date.now() - started).toBeLessThan(5000);
  }
  await expect(card.getByText('Turned off after errors')).toBeVisible();
  await expect(page.getByText(/Hostile was turned off/).first()).toBeVisible();
  await expect(
    page
      .locator('article', { hasText: 'Estimate vs actual' })
      .getByText('Running'),
  ).toBeVisible();
});

const boardBlocking = async (page, shell, { safeMode }) => {
  await page.addInitScript((safe) => {
    localStorage.setItem('plugins.safeMode', JSON.stringify(safe));
    window.__longTasks = 0;
    new PerformanceObserver((list) =>
      list
        .getEntries()
        .forEach(
          (entry) => (window.__longTasks += Math.max(0, entry.duration - 50)),
        ),
    ).observe({ type: 'longtask', buffered: true });
  }, safeMode);
  await page.goto('/local-personal/board');
  await expect(page.getByText('Task 499').first()).toBeAttached();
  if (!safeMode) await expect(page.getByText('150%').first()).toBeVisible();
  await page.waitForTimeout(1500);
  return page.evaluate(() => window.__longTasks);
};

test('badges for a 500-task board add little main-thread work', async ({
  page,
}) => {
  const shell = await desktopPage(page);
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  const [backlog] = await shell('local_db_select', {
    code: 'personal',
    sql: `SELECT id FROM statuses WHERE type = 'default'`,
    params: [],
  });
  for (let i = 0; i < 500; i++) {
    await shell('local_db_execute', {
      code: 'personal',
      sql: `INSERT INTO tasks (title, status_id, common_time, approximately_time, created_at, updated_at)
            VALUES (?, ?, 10800, 7200, '', '')`,
      params: [`Task ${i}`, backlog.id],
    });
  }

  const baseline = await boardBlocking(page, shell, { safeMode: true });
  const withPlugin = await boardBlocking(page, shell, { safeMode: false });
  console.log(
    `long-task blocking over 50 ms: safe mode ${Math.round(
      baseline,
    )} ms, with plugin ${Math.round(withPlugin)} ms`,
  );
  expect(withPlugin - baseline).toBeLessThan(300);
});

test('a folder plugin talks to a local service it declared, and the settings say so in red', async ({
  page,
}) => {
  await page.addInitScript(() =>
    localStorage.setItem('plugins.devMode', 'true'),
  );
  const llm = {
    folder: 'dev.llm',
    manifest: JSON.stringify({
      id: 'dev.llm',
      name: 'Local LLM',
      version: '1.0.0',
      engines: { tmgr: '^1.0' },
      permissions: ['notifications'],
      network: { allowedOrigins: ['http://localhost:11434'] },
      contributes: {
        commands: [{ id: 'dev.llm.ask', title: 'Ask the model' }],
      },
    }),
    code: `
      tmgr.commands.register('dev.llm.ask', async () => {
        const res = await tmgr.net.fetch('http://localhost:11434/api/generate', { method: 'POST', body: '{"prompt":"hi"}' });
        const answer = (await res.json()).response;
        const refused = await tmgr.net.fetch('http://localhost:22/').then(() => 'allowed', (e) => e.name);
        await tmgr.ui.notify(answer + ' / port 22: ' + refused);
      });
    `,
  };
  const shell = await desktopPage(
    page,
    {},
    {
      devPlugins: [llm],
      localHttp: () => ({
        status: 200,
        headers: [['content-type', 'application/json']],
        body: '{"response":"hello from the model"}',
      }),
    },
  );
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);

  await page.locator('[data-sidebar="footer"] button').first().click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();
  const card = page.locator('article', { hasText: 'Local LLM' });
  await expect(
    card.getByText(
      'Can connect to: http://localhost:11434 (this computer only)',
    ),
  ).toBeVisible();
  await card.getByRole('switch').click();
  await expect(card.getByText('Running')).toBeVisible();
  await card.getByRole('button', { name: 'Ask the model' }).click();
  await expect(
    page.getByText('hello from the model / port 22: PERMISSION_DENIED', {
      exact: true,
    }),
  ).toBeVisible();
  expect(shell.fetches).toEqual([
    {
      url: 'http://localhost:11434/api/generate',
      method: 'POST',
      headers: [],
      body: '{"prompt":"hi"}',
    },
  ]);
});
