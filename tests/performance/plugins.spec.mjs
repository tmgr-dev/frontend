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

  await card.locator('details summary').click();
  const timeouts = async () =>
    (
      (await card.locator('details').innerText()).match(/ran longer than/g) ??
      []
    ).length;
  for (let i = 0; i < 3; i++) {
    await card.getByRole('button', { name: 'Spin forever' }).click();
    expect(await page.evaluate(() => performance.now())).toBeGreaterThan(0);
    await expect.poll(timeouts).toBe(i + 1);
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

test('a folder plugin exports into its own folder, reads an attachment and a file the user picks', async ({
  page,
}) => {
  await page.addInitScript(() =>
    localStorage.setItem('plugins.devMode', 'true'),
  );
  const plugin = {
    folder: 'dev.files',
    manifest: JSON.stringify({
      id: 'dev.files',
      name: 'Files',
      version: '1.0.0',
      engines: { tmgr: '^1.0' },
      permissions: [
        'notifications',
        'files:export',
        'files:attachments',
        'files:pick',
      ],
      contributes: { commands: [{ id: 'dev.files.go', title: 'Use files' }] },
    }),
    code: `
      tmgr.commands.register('dev.files.go', async () => {
        await tmgr.files.export('summary/today.md', '# Today');
        const [file] = await tmgr.files.list(1);
        const attachment = await tmgr.files.read(file.id);
        const picked = await tmgr.files.pick();
        const escape = await tmgr.files.export('../../outside.md', 'x').then(() => 'written', (e) => e.name);
        await tmgr.ui.notify([file.name, attachment.text, picked.text, escape].join(' | '));
      });
    `,
  };
  const shell = await desktopPage(page, {}, { devPlugins: [plugin] });
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  await shell('local_db_execute', {
    code: 'personal',
    sql: `INSERT INTO tasks (title, created_at, updated_at) VALUES ('With a file', '', '')`,
    params: [],
  });
  await shell('local_db_execute', {
    code: 'personal',
    sql: `INSERT INTO files (task_id, name, file_path, mime_type, size, created_at) VALUES (1, 'notes.md', 'abc/notes.md', 'text/markdown', 7, '')`,
    params: [],
  });
  shell.files.set('personal/abc/notes.md', Buffer.from('# Notes'));

  await page.locator('[data-sidebar="footer"] button').first().click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();
  const card = page.locator('article', { hasText: 'read a file you choose' });
  await expect(
    card.getByText("save files to this workspace's exports folder"),
  ).toBeVisible();
  await card.getByRole('switch').click();
  await expect(card.getByText('Running')).toBeVisible();
  await card.getByRole('button', { name: 'Use files' }).click();
  await expect(
    page.getByText('notes.md | # Notes | hi | INVALID_PARAMS', { exact: true }),
  ).toBeVisible();
  expect(shell.exports).toEqual([
    {
      code: 'personal',
      folder: 'plugins/dev.files',
      files: [{ path: 'summary/today.md', content: '# Today' }],
    },
  ]);
  expect(shell.picks).toEqual(['Choose a file for the Files plugin']);
});

test('a plugin view with its own page opens in a window whose calls go through the plugin broker', async ({
  page,
}) => {
  await page.addInitScript(() =>
    localStorage.setItem('plugins.devMode', 'true'),
  );
  const plugin = {
    folder: 'dev.win',
    manifest: JSON.stringify({
      id: 'dev.win',
      name: 'Window plugin',
      version: '1.0.0',
      engines: { tmgr: '^1.0' },
      permissions: ['tasks:read'],
      contributes: {
        commands: [{ id: 'dev.win.hello', title: 'Hello' }],
        views: [{ id: 'board', title: 'Big board', ui: 'ui/board.html' }],
      },
    }),
    code: `tmgr.commands.register('dev.win.hello', (args) => 'hello ' + args.name);`,
    pages: [['ui/board.html', '<h1>Big board</h1>']],
  };
  const shell = await desktopPage(page, {}, { devPlugins: [plugin] });
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);

  await page.locator('[data-sidebar="footer"] button').first().click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();
  await page
    .locator('article', { hasText: 'Window plugin' })
    .getByRole('switch')
    .click();
  await page.getByRole('link', { name: 'Big board' }).first().click();
  await expect(page.getByRole('button', { name: 'Open window' })).toBeVisible();
  await expect.poll(() => shell.windows.length).toBe(1);
  expect(shell.windows[0]).toMatchObject({
    key: 'dev.win/board',
    title: 'Big board',
  });
  const generation = shell.windows[0].generation;
  expect(shell.pages.get('dev.win/board')).toBe('<h1>Big board</h1>');

  const call = (call_id, method, params) =>
    page.evaluate((payload) => window.__emit('plugin-window://call', payload), {
      call_id,
      plugin_id: 'dev.win',
      generation,
      method,
      params,
    });
  await call(1, 'tasks.list', {});
  await call(2, 'commands.run', { id: 'dev.win.hello', args: { name: 'Ann' } });
  await call(3, 'tasks.update', { id: 1, patch: { title: 'x' } });
  await call(4, 'register', { kind: 'command', id: 'dev.win.hello' });
  await expect.poll(() => shell.replies.length).toBe(4);
  const byId = Object.fromEntries(shell.replies.map((r) => [r.callId, r]));
  expect(byId[1]).toMatchObject({ ok: true, value: { items: [], total: 0 } });
  expect(byId[2]).toEqual({ callId: 2, ok: true, value: 'hello Ann' });
  expect(byId[3]).toMatchObject({
    ok: false,
    value: expect.stringContaining('PERMISSION_DENIED'),
  });
  expect(byId[4]).toMatchObject({
    ok: false,
    value: expect.stringContaining('UNKNOWN_METHOD'),
  });

  await page.locator('[data-sidebar="footer"] button').first().click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();
  await page
    .locator('article', { hasText: 'Window plugin' })
    .getByRole('switch')
    .click();
  await expect.poll(() => shell.closed).toContain('dev.win');
});

test('a plugin is installed from a GitHub release only after the user agrees, and an update shows new permissions', async ({
  page,
}) => {
  const releases = {
    'acme/board': {
      tag: 'v1.0.0',
      content: {
        manifest: {
          id: 'acme.board',
          name: 'Acme board',
          version: '1.0.0',
          engines: { tmgr: '^1.0' },
          description: 'A board overview',
          permissions: ['tasks:read'],
          contributes: { commands: [{ id: 'acme.board.ping', title: 'Ping' }] },
        },
        code: `tmgr.commands.register('acme.board.ping', () => 'pong');`,
      },
    },
  };
  const shell = await desktopPage(page, {}, { releases });
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  await page.locator('[data-sidebar="footer"] button').first().click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();

  await page
    .getByLabel('Plugin repository')
    .fill('https://github.com/acme/missing');
  await page.getByRole('button', { name: 'Check', exact: true }).click();
  await expect(
    page.getByText(
      'the latest release of https://github.com/acme/missing has no tmgr-plugin.json',
    ),
  ).toBeVisible();

  await page.getByLabel('Plugin repository').fill('acme/board');
  await page.getByRole('button', { name: 'Check', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('Install Acme board 1.0.0?')).toBeVisible();
  await expect(dialog.getByText('read tasks')).toBeVisible();
  await expect(dialog.getByText(/Unverified publisher/)).toBeVisible();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  expect(shell.installed.size).toBe(0);

  await page.getByRole('button', { name: 'Check', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Install' })
    .click();
  const card = page.locator('article', { hasText: 'Acme board' });
  await expect(
    card.getByText('From github.com/acme/board · v1.0.0'),
  ).toBeVisible();
  await expect(card.getByText('Off')).toBeVisible();
  await card.getByRole('switch').click();
  await expect(card.getByText('Running')).toBeVisible();
  await expect(card.getByText(/unverified publisher/)).toBeVisible();

  releases['acme/board'] = {
    tag: 'v1.1.0',
    content: {
      ...releases['acme/board'].content,
      manifest: {
        ...releases['acme/board'].content.manifest,
        version: '1.1.0',
        permissions: ['tasks:read', 'tasks:write'],
      },
    },
  };
  await card.getByRole('button', { name: 'Check for update' }).click();
  const update = page.getByRole('dialog');
  await expect(update.getByText('Update Acme board 1.1.0?')).toBeVisible();
  await expect(
    update.getByText('New in this version: create and change tasks'),
  ).toBeVisible();
  await update.getByRole('button', { name: 'Update' }).click();
  await expect(
    card.getByText('From github.com/acme/board · v1.1.0'),
  ).toBeVisible();

  releases['mallory/board'] = {
    tag: 'v9',
    content: releases['acme/board'].content,
  };
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.getByLabel('Plugin repository').fill('mallory/board');
  await page.getByRole('button', { name: 'Check', exact: true }).click();
  await expect(
    page.getByText(
      'acme.board is already installed from github.com/acme/board',
    ),
  ).toBeVisible();

  await card.getByRole('button', { name: 'Remove' }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Remove' })
    .click();
  await expect(page.locator('article', { hasText: 'Acme board' })).toHaveCount(
    0,
  );
  expect(shell.installed.size).toBe(0);

  await page.getByLabel('Plugin repository').fill('acme/board');
  await page.getByRole('button', { name: 'Check', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Install' })
    .click();
  await expect(
    page.locator('article', { hasText: 'Acme board' }).getByText('Off'),
  ).toBeVisible();
});

test('the signed TMGR blocklist stops an installed plugin and a built-in one', async ({
  page,
}) => {
  const releases = {
    'acme/board': {
      tag: 'v1.0.0',
      verified: true,
      content: {
        manifest: {
          id: 'acme.board',
          name: 'Acme board',
          version: '1.0.0',
          engines: { tmgr: '^1.0' },
          permissions: ['tasks:read'],
        },
        code: '',
      },
    },
  };
  const shell = await desktopPage(page, {}, { releases });
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  await page.locator('[data-sidebar="footer"] button').first().click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();

  await page.getByLabel('Plugin repository').fill('acme/board');
  await page.getByRole('button', { name: 'Check', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText(/Verified publisher/)).toBeVisible();
  await dialog.getByRole('button', { name: 'Install' }).click();
  const card = page.locator('article', { hasText: 'Acme board' });
  await card.getByRole('switch').click();
  await expect(card.getByText('Running')).toBeVisible();
  const estimate = page.locator('article', { hasText: 'Estimate' }).first();
  await expect(estimate.getByText('Running')).toBeVisible();

  shell.catalog.serial = 2;
  shell.catalog.blocked.push(
    { repo: 'acme/board', reason: 'compromised release' },
    { id: 'tmgr.estimate', reason: 'broken build' },
  );
  await page.reload();
  await expect(card.getByText('Blocked by TMGR')).toBeVisible();
  await expect(card.getByText('compromised release')).toBeVisible();
  await expect(card.getByRole('switch')).toBeDisabled();
  await expect(estimate.getByText('Blocked by TMGR')).toBeVisible();
  await expect(estimate.getByText('broken build')).toBeVisible();
});

const openPluginSettings = async (page) => {
  await page.locator('[data-sidebar="footer"] button').first().click();
  await page.getByRole('menuitem', { name: 'Plugins' }).click();
};

const sharedWorkspacePlugins = async (page, records) => {
  const minted = [];
  await page.route('**/api/workspaces/1/plugins**', async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname.replace('/api/', '');
    if (path.endsWith('/token')) {
      minted.push(request.headers().authorization);
      await route.fulfill({
        json: { data: { token: 'plugin-token', expires_in: 900 } },
      });
      return;
    }
    const pluginId = path.split('/')[3];
    if (request.method() === 'PUT')
      records.push({ plugin_id: pluginId, ...request.postDataJSON(), enabled_by: 1 });
    if (request.method() === 'DELETE')
      records.splice(records.findIndex((r) => r.plugin_id === pluginId), 1);
    await route.fulfill({
      json: { data: request.method() === 'GET' ? records : { success: true } },
    });
  });
  const pluginCalls = [];
  page.on('request', (request) => {
    if (request.headers().authorization === 'Bearer plugin-token')
      pluginCalls.push(new URL(request.url()));
  });
  return { minted, pluginCalls };
};

test('the creator of a shared workspace turns a plugin on for everyone; it talks to the server with its own token', async ({
  page,
}) => {
  await desktopPage(page);
  const records = [];
  const { minted, pluginCalls } = await sharedWorkspacePlugins(page, records);
  await page.goto('/demo/board');
  await openPluginSettings(page);
  await expect(page.getByText(/You created it/)).toBeVisible();
  const card = page.locator('article', { hasText: 'Estimate vs actual' });
  await expect(card.getByText('Off')).toBeVisible();

  await card.getByRole('switch').click();
  await expect(card.getByText('Running')).toBeVisible();
  expect(records).toEqual([
    expect.objectContaining({
      plugin_id: 'tmgr.estimate',
      repo: 'builtin',
      sha256: null,
      public_key: null,
      permissions: ['tasks:read', 'statuses:read', 'time:read'],
    }),
  ]);

  await page.goBack();
  await expect(page).toHaveURL(/\/demo\/board/);
  await expect.poll(() => pluginCalls.length).toBeGreaterThan(0);
  expect(minted.length).toBeGreaterThan(0);
  expect(minted.every((header) => header === 'Bearer fixture')).toBe(true);
  const lists = pluginCalls.filter((url) =>
    /\/api\/(tasks|workspaces\/statuses)$/.test(url.pathname),
  );
  expect(lists.length).toBeGreaterThan(0);
  expect(lists.every((url) => url.searchParams.get('workspace_id') === '1')).toBe(
    true,
  );

  await openPluginSettings(page);
  await card.getByRole('switch').click();
  await expect(card.getByText('Off')).toBeVisible();
  expect(records).toEqual([]);
});

test('a member gets the plugins the creator turned on, and allows network and files on their computer', async ({
  page,
}) => {
  const net = {
    tag: 'v1.0.0',
    content: {
      manifest: {
        id: 'acme.net',
        name: 'Acme net',
        version: '1.0.0',
        engines: { tmgr: '^1.0' },
        permissions: ['notifications'],
        network: { allowedOrigins: ['http://localhost:11434'] },
        contributes: { commands: [{ id: 'acme.net.ask', title: 'Ask' }] },
      },
      code: `tmgr.commands.register('acme.net.ask', async () => {
        const answer = await tmgr.net.fetch('http://localhost:11434/x').then((r) => r.text(), (e) => e.message);
        await tmgr.ui.notify('answer: ' + answer);
      });`,
    },
  };
  const { createHash } = await import('node:crypto');
  const shell = await desktopPage(
    page,
    {},
    {
      releases: { 'acme/net': net },
      localHttp: () => ({ status: 200, headers: [], body: 'from this computer' }),
    },
  );
  await page.route('**/api/workspaces', (route) =>
    route.fulfill({
      json: {
        data: [{ id: 1, name: 'Demo', code: 'demo', user_id: 2, is_default: true }],
      },
    }),
  );
  const records = [
    {
      plugin_id: 'tmgr.estimate',
      repo: 'builtin',
      version: '1.0.0',
      sha256: null,
      public_key: null,
      permissions: ['tasks:read', 'statuses:read', 'time:read'],
      enabled_by: 2,
    },
    {
      plugin_id: 'acme.net',
      repo: 'acme/net',
      version: 'v1.0.0',
      sha256: createHash('sha256').update(JSON.stringify(net.content)).digest('hex'),
      public_key: 'RWtestkey',
      permissions: ['notifications'],
      enabled_by: 2,
    },
  ];
  await sharedWorkspacePlugins(page, records);
  await page.goto('/demo/board');
  await openPluginSettings(page);
  await expect(page.getByText(/Only its creator turns plugins on/)).toBeVisible();
  const estimate = page.locator('article', { hasText: 'Estimate vs actual' });
  await expect(estimate.getByText('Running')).toBeVisible();
  await expect(estimate.getByRole('switch')).toBeDisabled();
  await expect(estimate.getByText(/Allow on this computer/)).toHaveCount(0);

  const card = page.locator('article', { hasText: 'Acme net' });
  await expect(card.getByText('Running')).toBeVisible();
  expect(shell.installed.has('acme.net')).toBe(true);
  await card.getByRole('button', { name: 'Ask' }).click();
  await expect(page.getByText(/answer: network access is not available/).first()).toBeVisible();
  expect(shell.fetches).toEqual([]);

  await card.getByRole('button', { name: 'Allow on this computer' }).click();
  await expect(card.getByText(/Allowed to use the network/)).toBeVisible();
  await expect(card.getByText('Running')).toBeVisible();
  await card.getByRole('button', { name: 'Ask' }).click();
  await expect(page.getByText('answer: from this computer').first()).toBeVisible();
  expect(shell.fetches.map((f) => f.url)).toEqual(['http://localhost:11434/x']);
});
