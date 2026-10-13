import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const screensDir = process.env.SCREENS_DIR;
if (screensDir) mkdirSync(screensDir, { recursive: true });

const WS = -1000;
const READY = { state: 'ready', version: '0.4.0', caps: ['runs.create'] };
const PROFILES = [
  { id: 'claude-code', label: 'Claude Code' },
  { id: 'codex', label: 'Codex' },
];

const session = (id, taskId, kind, detail = null, over = {}) => ({
  id,
  name: over.name ?? `Task ${taskId}`,
  profile: 'claude-code',
  task: { workspaceId: WS, taskId },
  state: { kind, sinceMs: Date.now(), detail },
  createdMs: Date.now() - 18 * 60_000,
  ...over,
});

/** A scripted runner: answers the runner_* commands and pushes channel messages into the page. */
const makeRunner = (page, init = {}) => {
  const st = {
    enabled: init.enabled ?? true,
    status: init.status ?? READY,
    sessions: init.sessions ?? [],
    cancelStart: init.cancelStart ?? false,
    calls: [],
    watchChannels: [],
    frameChannel: null,
    termInfo: init.termInfo ?? null,
  };
  const indexes = new Map();
  const channelId = (value) =>
    typeof value === 'object' ? value.id : Number(String(value).split(':')[1]);
  st.push = async (id, message) => {
    const index = indexes.get(id) ?? 0;
    indexes.set(id, index + 1);
    await page.evaluate(
      ({ id, message, index }) => window[`__callback${id}`]({ index, message }),
      { id, message, index },
    );
  };
  st.pushSessions = async () => {
    await expect
      .poll(() =>
        page.evaluate(
          (id) => typeof window[`__callback${id}`] === 'function',
          st.watchChannels[0],
        ),
      )
      .toBe(true);
    for (const id of st.watchChannels)
      await st.push(id, { type: 'sessions', sessions: st.sessions });
  };
  st.frame = (message) => st.push(st.frameChannel, message);
  st.handler = async (command, args = {}) => {
    st.calls.push({ command, args });
    switch (command) {
      case 'runner_enabled':
        return st.enabled;
      case 'runner_set_enabled':
        st.enabled = args.enabled;
        return null;
      case 'runner_status':
        return st.enabled ? st.status : { state: 'disabled', caps: [] };
      case 'runner_profiles':
        return PROFILES;
      case 'runner_sessions':
        return st.sessions;
      case 'runner_watch':
        st.watchChannels = [channelId(args.onEvent)];
        indexes.delete(st.watchChannels[0]);
        return 1;
      case 'runner_unwatch':
        return null;
      case 'runner_start_run': {
        if (st.cancelStart) throw 'cancelled';
        const created = session(
          `s_new${st.sessions.length}`,
          args.taskId,
          'starting',
          null,
          { name: args.taskTitle, profile: args.profile, createdMs: Date.now() },
        );
        st.sessions = [...st.sessions, created];
        return created;
      }
      case 'runner_term_attach':
        st.frameChannel = channelId(args.onFrame);
        indexes.delete(st.frameChannel);
        return st.termInfo;
      default:
        return null;
    }
  };
  st.named = (command) => st.calls.filter((call) => call.command === command);
  return st;
};

const setup = async (page, { runner: runnerInit, dark = false } = {}) => {
  const runner = makeRunner(page, runnerInit);
  const shell = await desktopPage(
    page,
    { account: false },
    {
      runner: runner.handler,
      profile: {
        id: -1,
        uuid: '00000000-0000-4000-8000-000000000001',
        name: 'Ada',
        created_at: '2026-10-11T10:00:00Z',
      },
    },
  );
  await page.addInitScript(
    (scheme) => localStorage.setItem('colorScheme', scheme),
    dark ? 'dark' : 'light',
  );
  await page.emulateMedia({ colorScheme: dark ? 'dark' : 'light' });
  return { runner, shell };
};

const addTask = async (page, title) => {
  await page
    .getByRole('button', { name: 'Open Add Task Modal' })
    .first()
    .click();
  await page.getByPlaceholder('Task name').fill(title);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect(page.getByText(title).first()).toBeVisible();
};

const seedTasks = async (page, titles) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/local-my-workspace\//);
  await page.goto('/local-my-workspace/board');
  for (const title of titles) {
    await addTask(page, title);
    await page.goto('/local-my-workspace/board');
  }
};

const ids = async (shell) =>
  (
    await shell('local_db_select', {
      code: 'my-workspace',
      sql: 'SELECT id, title FROM tasks ORDER BY id',
      params: [],
    })
  ).map((row) => Number(row.id));

const openTask = async (page, title) => {
  await page.getByText(title, { exact: true }).first().click();
  await expect(page.getByPlaceholder('Task name')).toBeVisible();
};

test.use({ viewport: { width: 1440, height: 900 } });

test('flag off: no button, no chips, switch is off', async ({ page }) => {
  const { runner } = await setup(page, { runner: { enabled: false } });
  await seedTasks(page, ['Alpha task']);
  await expect(page.locator('[data-agent-state]')).toHaveCount(0);
  await openTask(page, 'Alpha task');
  await expect(
    page.getByRole('button', { name: 'Run with agent' }),
  ).toHaveCount(0);
  await expect(page.getByTestId('task-agent-runner')).toHaveCount(0);
  expect(runner.named('runner_watch')).toHaveLength(0);
  expect(runner.named('runner_status')).toHaveLength(0);

  await page.goto('/settings?tab=experimental');
  await expect(page.getByText('Agent runner').first()).toBeVisible();
  await expect(page.getByTestId('runner-switch')).toHaveAttribute(
    'aria-checked',
    'false',
  );
  await expect(page.getByTestId('runner-status')).toHaveCount(0);
});

test('enabling from Settings starts the watch and shows status', async ({
  page,
}) => {
  const { runner } = await setup(page, { runner: { enabled: false } });
  await page.goto('/settings?tab=experimental');
  await page.getByTestId('runner-switch').click();
  await expect(page.getByTestId('runner-switch')).toHaveAttribute(
    'aria-checked',
    'true',
  );
  await expect(page.getByTestId('runner-status')).toHaveText('Ready v0.4.0');
  await expect(page.getByTestId('runner-permissions')).toHaveText('on');
  await expect(page.getByText('tmgr config allow-bypass')).toBeVisible();
  await expect(page.getByTestId('bypass-open')).toHaveCount(0);
  expect(runner.named('runner_set_enabled')[0].args).toEqual({
    enabled: true,
  });
  expect(runner.named('runner_watch')).toHaveLength(1);
  await expect(page.getByTestId('agent-sessions-empty')).toBeVisible();

  await page.getByTestId('runner-switch').click();
  await expect(page.getByTestId('runner-status')).toHaveCount(0);
  expect(runner.named('runner_unwatch')).toHaveLength(1);
});

test('runner status line: absent', async ({ page }) => {
  await setup(page, { runner: { status: { state: 'absent', caps: [] } } });
  await page.goto('/settings?tab=experimental');
  await expect(page.getByTestId('runner-status')).toHaveText('Not running');
  await expect(page.getByTestId('agent-sessions-error')).toContainText(
    'tmgr runner start',
  );
});

test('bypass dialog appears only with runs.bypass and needs the typed phrase', async ({
  page,
}) => {
  await setup(page, {
    runner: { status: { ...READY, caps: ['runs.create', 'runs.bypass'] } },
  });
  await page.goto('/settings?tab=experimental');
  await page.getByTestId('bypass-open').click();
  const confirm = page.getByTestId('bypass-confirm');
  await expect(confirm).toBeDisabled();
  await page.getByTestId('bypass-phrase').fill('i agree');
  await expect(confirm).toBeDisabled();
  await page.getByTestId('bypass-phrase').fill('I AGREE');
  await expect(confirm).toBeEnabled();
});

test('board chips, Needs input highlight, task view run flow', async ({
  page,
}) => {
  const { runner, shell } = await setup(page);
  await seedTasks(page, [
    'Pay button',
    'Image resize',
    'Footer links',
    'Fresh task',
  ]);
  const [a, b, c, d] = await ids(shell);
  runner.sessions = [
    session('s_work', a, 'working', null, { name: 'Pay button' }),
    session('s_blocked', b, 'blocked', 'Run rm -rf build?', {
      name: 'Image resize',
    }),
    session('s_done', c, 'exited', 'done', {
      name: 'Footer links',
      profile: 'codex',
    }),
  ];
  await runner.pushSessions();

  await expect(page.locator('[data-agent-state="working"]')).toHaveText(
    'Working',
  );
  const blocked = page.locator('[data-agent-state="blocked"]');
  await expect(blocked).toHaveText('Needs input');
  await expect(blocked).toHaveAttribute('data-attention', 'true');
  await expect(page.locator('[data-agent-state="exited"]')).toHaveText('Done');
  await expect(page.getByText('rm -rf')).toHaveCount(0);

  await openTask(page, 'Pay button');
  await expect(page.getByTestId('agent-open-terminal').first()).toBeVisible();
  await page.getByTestId('agent-open-terminal').first().click();
  await expect.poll(() => runner.named('runner_open_terminal').length).toBe(1);
  expect(runner.named('runner_open_terminal')[0].args).toEqual({
    session: 's_work',
  });
  await page.keyboard.press('Escape');

  await page.goto('/local-my-workspace/board');
  await openTask(page, 'Fresh task');
  const run = page.getByTestId('agent-run-button');
  await expect(run).toHaveText(/Run with agent/);
  await run.click();
  await expect(page.getByTestId('agent-permission-note')).toHaveText(
    'Permission prompts: on',
  );
  await page.getByTestId('agent-profile').selectOption('codex');
  runner.cancelStart = true;
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect.poll(() => runner.named('runner_start_run').length).toBe(1);
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByTestId('agent-run-popover')).toBeVisible();
  runner.cancelStart = false;
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect.poll(() => runner.named('runner_start_run').length).toBe(2);
  expect(runner.named('runner_start_run')[1].args).toEqual({
    workspaceId: WS,
    taskId: d,
    taskTitle: 'Fresh task',
    profile: 'codex',
  });
  await runner.pushSessions();
  await expect(page.getByTestId('agent-sessions-panel')).toBeVisible();

  await page.goto('/settings?tab=experimental');
  await expect(page.locator('[data-session]')).toHaveCount(4);
  await page
    .locator('[data-session="s_work"]')
    .getByTestId('agent-stop')
    .click();
  await expect.poll(() => runner.named('runner_halt').length).toBe(1);
  expect(runner.named('runner_halt')[0].args).toEqual({ session: 's_work' });
  await expect(
    page.locator('[data-session="s_done"]').getByTestId('agent-stop'),
  ).toHaveCount(0);
});

test('runner absent: button disabled with the start hint', async ({ page }) => {
  await setup(page, { runner: { status: { state: 'absent', caps: [] } } });
  await seedTasks(page, ['Lonely task']);
  await openTask(page, 'Lonely task');
  const run = page.getByTestId('agent-run-button');
  await expect(run).toHaveAttribute('aria-disabled', 'true');
  await expect(run).toHaveAttribute(
    'title',
    'TMGR runner is not running. Start it with `tmgr runner start`.',
  );
  await run.click({ force: true });
  await expect(page.getByTestId('agent-run-popover')).toHaveCount(0);
});

test('cloud workspace task: button disabled with explanation', async ({
  page,
}) => {
  const runner = makeRunner(page);
  await desktopPage(page, {}, { runner: runner.handler });
  await page.goto('/demo/board');
  await page.getByText('Original task', { exact: true }).first().click();
  const run = page.getByTestId('agent-run-button');
  await expect(run).toBeVisible();
  await expect(run).toHaveAttribute('aria-disabled', 'true');
  await expect(run).toHaveAttribute(
    'title',
    'Agent runs work only with local workspaces for now.',
  );
});

const termPage = async (page, { dark = false } = {}) => {
  const runner = makeRunner(page, {
    termInfo: {
      session: 's_work',
      name: 'Pay button',
      task: null,
      state: { kind: 'working', sinceMs: 1, detail: null },
    },
  });
  await desktopPage(
    page,
    { account: false },
    { runner: runner.handler, windowLabel: 'runner-s_work' },
  );
  await page.addInitScript(() => {
    window.__clipboard = [];
    Object.defineProperty(navigator, 'clipboard', {
      value: {
        writeText: (text) => window.__clipboard.push(text),
        readText: async () => '',
      },
    });
  });
  await page.emulateMedia({ colorScheme: dark ? 'dark' : 'light' });
  await page.goto('/runner-terminal.html');
  await expect.poll(() => runner.frameChannel).toBeGreaterThan(0);
  return runner;
};

const SCREEN =
  '\x1b[H\x1b[2J\x1b[1;1H\x1b[0;1mReading src/checkout/ApplePayButton.vue\x1b[0m\x1b[2;1H\x1b[0;32mWrite(src/checkout)\x1b[0m\x1b[3;1H> ';

test('terminal window renders frames, acks, ignores OSC 52, scrolls and detaches', async ({
  page,
}) => {
  const runner = await termPage(page);
  await expect(page.locator('.name')).toHaveText('Pay button');
  await expect(page.locator('.chip')).toHaveText('Working');
  expect(runner.named('runner_term_attach')[0].args.cols).toBeGreaterThan(10);

  await runner.frame({ type: 'data', seq: 1, data: SCREEN });
  await expect(page.locator('.xterm-rows')).toContainText(
    'Reading src/checkout/ApplePayButton.vue',
  );
  await expect
    .poll(() => runner.named('runner_term_ack').map((c) => c.args.seq))
    .toContain(1);

  await runner.frame({
    type: 'data',
    seq: 2,
    data: '\x1b]52;c;aGVsbG8=\x07\x1b[4;1Hdone',
  });
  await expect
    .poll(() => runner.named('runner_term_ack').map((c) => c.args.seq))
    .toContain(2);
  expect(await page.evaluate(() => window.__clipboard)).toEqual([]);

  await page.locator('#term').click();
  await page.keyboard.type('hi');
  await expect
    .poll(() =>
      runner
        .named('runner_term_input')
        .map((c) => c.args.data)
        .join(''),
    )
    .toBe('hi');

  await runner.frame({ type: 'scroll', offset: 0, max: 50 });
  await page.locator('#term').hover();
  await page.mouse.wheel(0, -100);
  await expect.poll(() => runner.named('runner_term_scroll').length).toBe(1);
  expect(runner.named('runner_term_scroll')[0].args).toEqual({ offset: 3 });
  await runner.frame({ type: 'scroll', offset: 3, max: 50 });
  await expect(page.locator('.scroll')).toHaveText(
    'Scrolled back 3 lines — press End to return',
  );
  await page.keyboard.press('End');
  await expect.poll(() => runner.named('runner_term_scroll').length).toBe(2);
  expect(runner.named('runner_term_scroll')[1].args).toEqual({ offset: 0 });
  await runner.frame({ type: 'scroll', offset: 0, max: 50 });
  await expect(page.locator('.scroll')).toBeHidden();

  await page.getByRole('button', { name: 'Detach' }).click();
  await expect.poll(() => runner.named('runner_term_detach').length).toBe(1);
});

test('terminal window: Needs input header and closed overlay with reattach', async ({
  page,
}) => {
  const runner = await termPage(page);
  await runner.frame({ type: 'data', seq: 1, data: SCREEN });
  await runner.frame({
    type: 'state',
    state: {
      kind: 'blocked',
      sinceMs: 2,
      detail: '<img src=x onerror=alert(1)>',
    },
  });
  await expect(page.locator('.chip')).toHaveText('Needs input');
  await expect(page.locator('.bar')).toHaveClass(/attention/);
  await expect(page.locator('.detail')).toHaveText(
    '<img src=x onerror=alert(1)>',
  );
  await expect(page.locator('.bar img')).toHaveCount(0);
  await runner.frame({ type: 'closed', reason: 'runner stopped' });
  await expect(page.locator('.overlay')).toContainText(
    'Session ended: runner stopped',
  );
  await page.getByRole('button', { name: 'Reattach' }).click();
  await expect.poll(() => runner.named('runner_term_attach').length).toBe(2);
  await expect(page.locator('.overlay')).toBeHidden();
});

test('terminal page ships a strict CSP', async ({ page }) => {
  await termPage(page);
  const csp = await page
    .locator('meta[http-equiv="Content-Security-Policy"]')
    .getAttribute('content');
  expect(csp).toContain("default-src 'self'");
  expect(csp).not.toMatch(/https?:\/\/(?!ipc\.localhost)/);
});

for (const [width, height] of [
  [1440, 900],
  [1000, 800],
]) {
  for (const dark of [false, true]) {
    const tag = `${width}-${dark ? 'dark' : 'light'}`;
    test(`screenshots ${tag}`, async ({ page }) => {
      test.skip(!screensDir, 'set SCREENS_DIR to capture');
      await page.setViewportSize({ width, height });
      const { runner, shell } = await setup(page, { dark });
      await seedTasks(page, [
        'Checkout: Apple Pay button',
        'Image pipeline: sharp resize',
        'Footer links audit',
        'Pricing page copy',
      ]);
      const [a, b, c] = await ids(shell);
      runner.sessions = [
        session('s_work', a, 'working', null, {
          name: 'Checkout: Apple Pay button',
        }),
        session('s_blocked', b, 'blocked', null, {
          name: 'Image pipeline: sharp resize',
        }),
        session('s_done', c, 'exited', 'done', {
          name: 'Footer links audit',
          profile: 'codex',
        }),
      ];
      await runner.pushSessions();
      await expect(page.locator('[data-agent-state]')).toHaveCount(3);
      await page.screenshot({
        path: join(screensDir, `board-chips-${tag}.png`),
      });

      await openTask(page, 'Pricing page copy');
      await page.getByTestId('agent-run-button').click();
      await expect(page.getByTestId('agent-profile')).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({
        path: join(screensDir, `task-run-popover-${tag}.png`),
      });
      await page.keyboard.press('Escape');
      await page.keyboard.press('Escape');

      await page.goto('/local-my-workspace/board');
      await openTask(page, 'Image pipeline: sharp resize');
      await expect(page.getByTestId('agent-sessions-panel')).toBeVisible();
      await page.getByTestId('agent-sessions-panel').scrollIntoViewIfNeeded();
      await page.screenshot({
        path: join(screensDir, `task-view-sessions-${tag}.png`),
      });

      await page.goto('/settings?tab=experimental');
      await expect(page.locator('[data-session]')).toHaveCount(3);
      await page.screenshot({
        path: join(screensDir, `settings-experimental-${tag}.png`),
      });
      runner.sessions = [];
      await runner.pushSessions();
      await expect(page.getByTestId('agent-sessions-empty')).toBeVisible();
      await page.screenshot({
        path: join(screensDir, `sessions-empty-${tag}.png`),
      });
    });

    test(`terminal screenshots ${tag}`, async ({ page }) => {
      test.skip(!screensDir, 'set SCREENS_DIR to capture');
      await page.setViewportSize({ width, height });
      const runner = await termPage(page, { dark });
      await runner.frame({ type: 'data', seq: 1, data: SCREEN });
      await expect(page.locator('.xterm-rows')).toContainText(
        'Reading src/checkout',
      );
      await page.screenshot({ path: join(screensDir, `terminal-${tag}.png`) });
      await runner.frame({
        type: 'state',
        state: {
          kind: 'blocked',
          sinceMs: 2,
          detail: 'Allow edit to ApplePayButton.vue?',
        },
      });
      await runner.frame({ type: 'scroll', offset: 12, max: 80 });
      await expect(page.locator('.bar')).toHaveClass(/attention/);
      await page.screenshot({
        path: join(screensDir, `terminal-needs-input-${tag}.png`),
      });
      await runner.frame({ type: 'closed', reason: 'runner stopped' });
      await page.screenshot({
        path: join(screensDir, `terminal-closed-${tag}.png`),
      });
    });
  }
}
