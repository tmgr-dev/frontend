import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';
import { SCHEMES, SIZES, mockOnboarding, setScheme, shoot } from './toursHelpers.mjs';

const e2e = process.env.S4_E2E === '1' ? test : test.skip;

const ALL_MODULES = [
  'agent_work',
  'alerts',
  'mcp',
  'ai.assistant',
  'personas',
  'task.assignees',
  'dashboard',
  'telegram',
  'board',
  'categories',
];

const runBody = (over = {}) => ({
  id: 7,
  task_id: 1,
  workspace_id: 1,
  user_id: 1,
  agent: 'claude-code',
  model: 'claude-opus-5-5',
  session_id: null,
  branch: 'feat/demo',
  status: 'running',
  started_at: new Date().toISOString(),
  ended_at: null,
  duration_seconds: 30,
  summary: 'Working on it',
  pr_url: null,
  commits: [],
  tests: null,
  version: 1,
  ...over,
});

const persona = {
  id: 'uuid-1',
  name: 'Reviewer',
  description: 'Reviews incoming tasks',
  avatar_url: null,
  system_prompt: 'You review.',
  prompt_version: 1,
  archived_at: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
  owner: { id: 1, name: 'Test User' },
};

const me = { id: 1, name: 'Test User', has_avatar: false };
const ann = { id: 2, name: 'Ann Lee', has_avatar: false };

const boot = async (page, size, { modules = ALL_MODULES, runs = [], personas = [persona], members = [me, ann] } = {}) => {
  await page.setViewportSize({ width: size.width, height: size.height });
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await mockApp(page, { multiStatus: true });
  const fixture = await mockOnboarding(page, { tips_mode: 'hub_only' });
  const calls = [];
  await page.route('**/api/workspaces/1/modules', (route) =>
    route.fulfill({
      json: {
        data: {
          configured: true,
          can_manage: true,
          packs: [{ key: 'main', name: 'Main' }],
          presets: [{ key: 'personal', name: 'Personal', description: 'Just you', modules: ['board'] }],
          modules: [...ALL_MODULES, 'graph'].map((key) => ({
            key,
            name: key,
            pack: 'main',
            enabled: modules.includes(key),
          })),
        },
      },
    }),
  );
  await page.route('**/api/tasks/1/agent-work', (route) =>
    route.fulfill({
      json: { data: { runs, totals: { agent_seconds: 30, human_seconds: 0, human_timer_running: false } } },
    }),
  );
  await page.route('**/api/notify-tokens*', (route) => {
    if (route.request().method() === 'POST') {
      calls.push('POST notify-tokens');
      return route.fulfill({
        json: { data: { id: 2, label: 'Claude Code on my laptop', prefix: 'tmgr_cd34', token: 'tmgrn_secret', created_at: '2026-01-01T00:00:00Z' } },
      });
    }
    return route.fulfill({ json: { data: [] } });
  });
  await page.route('**/api/personas*', (route) => {
    if (route.request().method() === 'POST') {
      calls.push('POST personas');
      return route.fulfill({ json: { data: persona } });
    }
    return route.fulfill({ json: { data: personas } });
  });
  await page.route('**/api/personas/*/tokens*', (route) => {
    if (route.request().method() === 'POST') {
      calls.push('POST persona-token');
      return route.fulfill({
        json: {
          data: {
            secret: 'tmgrp_secret',
            token: { id: 1, label: 'x', prefix: 'tmgrp', workspace_id: 1, workspace_name: 'Demo', expires_at: '2027-01-01T00:00:00Z', last_used_at: null, revoked_at: null },
          },
        },
      });
    }
    return route.fulfill({ json: { data: [] } });
  });
  await page.route('**/api/workspaces/1/personas*', (route) =>
    route.fulfill({
      json: {
        data: personas.map((p) => ({
          persona: { id: p.id, name: p.name, description: p.description, avatar_url: null, archived: false, owner: p.owner },
          workspace_id: 1,
          permissions: ['read'],
          blocked: false,
          blocked_at: null,
          effective_permissions: ['read'],
        })),
      },
    }),
  );
  await page.route('**/api/workspaces/1/assignable-personas*', (route) => route.fulfill({ json: { data: personas } }));
  await page.route('**/api/workspaces/1/members*', (route) => route.fulfill({ json: { data: members } }));
  await page.route('**/api/workspaces/1/dashboard/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('team-activity'))
      return route.fulfill({
        json: {
          data: {
            members: members.map((m) => ({
              id: m.id,
              name: m.name,
              email: `m${m.id}@example.test`,
              role: 'member',
              is_online: m.id === 1,
              last_activity_at: null,
              last_task: null,
              current_task: null,
              tracked_seconds: 3600,
              done_count: 2,
              active_tasks: 1,
              completed_tasks: 2,
              comments_count: 3,
              activity_count: 4,
              daily_activity: [],
              streak: 1,
            })),
            total_members: members.length,
            online_members: 1,
            active_today: 1,
            active_timers: 0,
            tracked_seconds: 3600,
            done_count: 2,
            window: '7d',
          },
        },
      });
    if (path.endsWith('heatmap'))
      return route.fulfill({
        json: { data: { contributions: [], total_contributions: 0, streak: { current: 0, longest: 0 }, year: 2026, weeks: 53 } },
      });
    if (path.endsWith('activities'))
      return route.fulfill({ json: { data: [], meta: { current_page: 1, per_page: 20, total: 0, last_page: 1 }, links: {} } });
    if (path.endsWith('recent-tasks')) return route.fulfill({ json: { data: [] } });
    if (path.endsWith('statistics'))
      return route.fulfill({
        json: {
          data: {
            total_tasks: 1, active_tasks: 1, completed_today: 0, completed_week: 0, completed_month: 0,
            time_today: 0, time_week: 0, time_month: 0, time_total: 0, team_members: members.length,
            overdue_tasks: 0, daily_routine_completion_rate: 0,
          },
        },
      });
    return route.fulfill({ json: { data: {} } });
  });
  await page.route('**/api/workspaces/1/invitations*', (route) => {
    if (route.request().method() === 'POST') calls.push('POST invitations');
    return route.fulfill({ json: { data: [] } });
  });
  await page.route('**/api/smart-device/**', (route) => {
    if (route.request().method() === 'POST') calls.push('POST smart-device');
    return route.fulfill({ json: { data: { token: 'device-token' } } });
  });
  await page.route('**/api/agent/conversations**', (route) => {
    const request = route.request();
    const url = request.url();
    if (request.method() === 'POST' && /\/messages$/.test(url)) {
      calls.push('POST message');
      return route.fulfill({ json: { data: { id: 2, role: 'assistant', content: 'Fine.', created_at: '2026-01-01T00:00:00Z' } } });
    }
    if (request.method() === 'POST') return route.fulfill({ json: { data: { id: 'conv-1' } } });
    return route.fulfill({ json: { data: [] } });
  });
  return { ...fixture, calls, errors };
};

const popover = (page) => page.locator('[role="dialog"][aria-labelledby^="tour-"]').first();

const lastWrite = (writes, id) => writes.filter((w) => w.path === `onboarding/tours/${id}`).at(-1)?.body;

const attached = async (page, anchors) => {
  for (const anchor of anchors) {
    await expect(page.locator(`[data-tour="${anchor}"]`).first(), anchor).toBeAttached();
  }
};

const startFromHub = async (page, id) => {
  await page.goto('/demo/getting-started');
  await page.getByTestId(`hub-start-${id}`).click();
  await expect(popover(page)).toBeVisible();
};

const expectClear = async (page, anchor) => {
  const target = page.locator(`[data-tour="${anchor}"]`).first();
  await expect(target).toBeInViewport();
  await page.waitForTimeout(900);
  const a = await target.boundingBox();
  const p = await popover(page).boundingBox();
  const vw = page.viewportSize();
  const x1 = Math.max(a.x, 0);
  const y1 = Math.max(a.y, 0);
  const x2 = Math.min(a.x + a.width, vw.width);
  const y2 = Math.min(a.y + a.height, vw.height);
  const visible = Math.max(0, x2 - x1) * Math.max(0, y2 - y1);
  const ox = Math.max(0, Math.min(x2, p.x + p.width) - Math.max(x1, p.x));
  const oy = Math.max(0, Math.min(y2, p.y + p.height) - Math.max(y1, p.y));
  expect(ox * oy, `${anchor} is mostly covered by the tour card`).toBeLessThan(visible * 0.6);
  expect(p.x, `${anchor} card is on screen`).toBeGreaterThanOrEqual(-1);
  expect(p.y, `${anchor} card is on screen`).toBeGreaterThanOrEqual(-1);
  expect(p.x + p.width).toBeLessThanOrEqual(vw.width + 1);
  expect(p.y + p.height).toBeLessThanOrEqual(vw.height + 1);
};

const fire = (page, name, payload = {}) =>
  page.evaluate(
    async ([n, p]) => {
      const { tourBus } = await import('/src/tours/events.ts');
      tourBus.emit(n, p);
    },
    [name, payload],
  );

const TOURS = [
  { id: 'agents.work-log', mobile: true, anchor: null },
  { id: 'agents.phone-alerts', mobile: true, url: /\/settings\/agent-notifications$/, anchor: 'agent-notify.connect' },
  { id: 'agents.mcp-token', mobile: false, url: /\/settings\?tab=device$/, anchor: 'settings.device-token' },
  { id: 'agents.ask-ai', mobile: true, anchor: 'nav.ask-ai' },
  { id: 'agents.connect-later', mobile: true, anchor: undefined },
  { id: 'personas.create', mobile: false, url: /\/settings\/personas$/, anchor: 'personas.create' },
  { id: 'personas.token', mobile: false, url: /\/settings\/personas$/, anchor: 'personas.tokens' },
  { id: 'personas.assign', mobile: false, anchor: null },
  { id: 'personas.skills', mobile: false, url: /\/settings\/personas$/, anchor: 'personas.skills' },
  { id: 'ws.modules', mobile: true, url: /\/settings\/modules$/, anchor: 'modules.presets' },
  { id: 'ws.invite', mobile: true, url: /\/settings\/workspaces$/, anchor: 'workspace.members' },
  { id: 'ws.dashboard', mobile: true, anchor: null },
  { id: 'ws.workspaces', mobile: true, anchor: 'nav.workspace-switcher', mobileAnchor: 'nav.menu-button' },
  { id: 'ws.appearance', mobile: true, url: /\/settings\?tab=theme$/, anchor: 'settings.theme' },
  { id: 'ws.notifications', mobile: true, anchor: 'nav.notification-bell' },
  { id: 'ws.privacy', mobile: true, url: /\/settings\?tab=privacy$/, anchor: 'settings.privacy-telemetry' },
  { id: 'ws.telegram', mobile: false, url: /\/settings\?tab=profile$/, anchor: 'settings.telegram' },
  { id: 'ws.default-landing', mobile: true, url: /\/settings\/modules$/, anchor: 'settings.default-landing' },
];

const GROUP_OF = (id) => id.split('.')[0];

for (const size of SIZES) {
  const mobile = size.width < 640;

  e2e(`hub lists S4a tours by platform and module at ${size.name}`, async ({ page }) => {
    const { errors } = await boot(page, size, { modules: ['board', 'categories'] });
    await page.goto('/demo/getting-started');
    for (const tour of TOURS) {
      const row = page.locator(`[data-tour-id="${tour.id}"]`);
      const platformOk = !mobile || tour.mobile;
      if (!platformOk) {
        await expect(row, `${tour.id} hidden by platform`).toHaveCount(0);
        continue;
      }
      const moduleless = ['agents.work-log', 'agents.phone-alerts', 'agents.mcp-token', 'agents.ask-ai', 'personas.create', 'personas.token', 'personas.assign', 'personas.skills', 'ws.dashboard', 'ws.telegram', 'agents.connect-later'];
      if (moduleless.includes(tour.id)) {
        await expect(row, `${tour.id} module off`).toContainText('Turn on');
        await expect(page.getByTestId(`hub-start-${tour.id}`)).toHaveCount(0);
      } else {
        await expect(page.getByTestId(`hub-start-${tour.id}`), `${tour.id} startable`).toBeVisible();
      }
    }
    for (const scheme of SCHEMES) await shoot(page, 'hub-modules-off', size, scheme);
    expect(errors).toEqual([]);
  });

  e2e(`hub lists every S4a tour as startable with all modules on at ${size.name}`, async ({ page }) => {
    const { errors } = await boot(page, size);
    await page.goto('/demo/getting-started');
    for (const tour of TOURS) {
      const start = page.getByTestId(`hub-start-${tour.id}`);
      if (mobile && !tour.mobile) await expect(start, `${tour.id} unavailable: platform`).toHaveCount(0);
      else await expect(start, tour.id).toBeVisible();
    }
    for (const scheme of SCHEMES) await shoot(page, 'hub-s4a', size, scheme);
    expect(errors).toEqual([]);
  });

  for (const tour of TOURS) {
    if (mobile && !tour.mobile) continue;
    e2e(`${tour.id} starts from the hub, lands on its route and anchor at ${size.name}`, async ({ page }) => {
      const { errors } = await boot(page, size);
      await startFromHub(page, tour.id);
      if (tour.url) await expect(page).toHaveURL(tour.url);
      if (tour.anchor === null) {
        await expect(popover(page)).toContainText('could not find this');
        await expect(popover(page).getByRole('button', { name: 'Take me there' })).toBeVisible();
      } else if (tour.anchor) {
        const anchor = mobile && tour.mobileAnchor ? tour.mobileAnchor : tour.anchor;
        await expect(popover(page)).not.toContainText('could not find this');
        await expect(page.locator(`[data-tour="${anchor}"]`).first()).toBeVisible();
        await expectClear(page, anchor);
      }
      for (const scheme of SCHEMES) await shoot(page, tour.id, size, scheme);
      await page.keyboard.press('Escape');
      expect(errors).toEqual([]);
    });
  }

  e2e(`persona tours fall back when there is no persona, take-me works at ${size.name}`, async ({ page }) => {
    test.skip(mobile, 'personas are web and desktop only');
    const { errors } = await boot(page, size, { personas: [] });
    await startFromHub(page, 'personas.token');
    await expect(page).toHaveURL(/\/settings\/personas$/);
    await expect(popover(page)).toContainText('could not find this');
    await shoot(page, 'personas.token-no-persona', size, 'light');
    await page.keyboard.press('Escape');
    expect(errors).toEqual([]);
  });

  e2e(`work-log: task opened, first run starts, run finishes, tour completes at ${size.name}`, async ({ page }) => {
    const { writes, errors } = await boot(page, size);
    await startFromHub(page, 'agents.work-log');
    await expect(popover(page)).toContainText('could not find this');
    await popover(page).getByRole('button', { name: 'Take me there' }).click();
    await expect(page).toHaveURL(/\/demo\/list$/);
    await page.locator('[data-task-id="1"]').first().click();
    await expect(page.locator('section[data-tour="task-form.agent-work"]')).toContainText('No agent runs yet');
    await expect(popover(page)).toContainText('Agent runs');
    await expect(popover(page)).not.toContainText('could not find this');
    await expectClear(page, 'task-form.agent-work');
    for (const scheme of SCHEMES) await shoot(page, 'agents.work-log-empty-state', size, scheme);
    await fire(page, 'agent.run_started', { task_id: 1 });
    await expect.poll(() => lastWrite(writes, 'agents.work-log')?.step_id).toBe('s2');
    await expect(popover(page)).toContainText('Run in progress');
    await shoot(page, 'agents.work-log-s2', size, 'light');
    await fire(page, 'agent.run_finished', { task_id: 1 });
    await expect.poll(() => lastWrite(writes, 'agents.work-log')?.status).toMatch(/completed|in_progress/);
    await expect.poll(() => lastWrite(writes, 'agents.work-log')?.step_id).not.toBe('s2');
    expect(errors).toEqual([]);
  });

  e2e(`work-log: a running run in the section shows the totals anchor at ${size.name}`, async ({ page }) => {
    const { errors } = await boot(page, size, { runs: [runBody()] });
    await page.goto('/demo/list');
    await page.locator('[data-task-id="1"]').first().click();
    await attached(page, ['task-form.agent-work', 'task-form.agent-totals']);
    expect(errors).toEqual([]);
  });

  e2e(`phone-alerts: create a token, dialog tips, completes at ${size.name}`, async ({ page }) => {
    const { writes, calls, errors } = await boot(page, size);
    await startFromHub(page, 'agents.phone-alerts');
    await page.getByPlaceholder('Claude Code on my laptop').fill('Claude Code on my laptop');
    await page.getByRole('button', { name: 'Create token' }).click();
    await expect.poll(() => calls.includes('POST notify-tokens')).toBe(true);
    await expect.poll(() => lastWrite(writes, 'agents.phone-alerts')?.step_id).toBe('s2');
    await attached(page, ['agent-notify.command', 'agent-notify.test']);
    await expect(popover(page)).toContainText('Add it to your agent');
    for (const scheme of SCHEMES) await shoot(page, 'agents.phone-alerts-dialog-command', size, scheme);
    await popover(page).getByRole('button', { name: 'Next' }).click();
    await expect(popover(page)).toContainText('Send a test');
    for (const scheme of SCHEMES) await shoot(page, 'agents.phone-alerts-dialog-test', size, scheme);
    await popover(page).getByRole('button', { name: 'Done' }).click();
    await expect.poll(() => lastWrite(writes, 'agents.phone-alerts')?.status).toBe('completed');
    expect(errors).toEqual([]);
  });

  e2e(`phone-alerts: skip this step on the wait step moves on at ${size.name}`, async ({ page }) => {
    const { writes, errors } = await boot(page, size);
    await startFromHub(page, 'agents.phone-alerts');
    await popover(page).getByRole('button', { name: /^Skip (this )?step$/ }).click();
    await expect.poll(() => lastWrite(writes, 'agents.phone-alerts')?.status, { timeout: 15000 }).toMatch(/completed|skipped/);
    expect(errors).toEqual([]);
  });

  e2e(`ask-ai: open the panel, ask a question, completes at ${size.name}`, async ({ page }) => {
    const { writes, calls, errors } = await boot(page, size);
    await startFromHub(page, 'agents.ask-ai');
    await expect(popover(page)).toContainText('Ask');
    for (const scheme of SCHEMES) await shoot(page, 'agents.ask-ai-s1', size, scheme);
    await page.locator('[data-tour="nav.ask-ai"]').first().click();
    await expect.poll(() => lastWrite(writes, 'agents.ask-ai')?.step_id).toBe('s2');
    await expect(page.locator('[data-tour="ai.input"]')).toBeVisible();
    for (const scheme of SCHEMES) await shoot(page, 'agents.ask-ai-s2', size, scheme);
    await page.locator('[data-tour="ai.input"]').fill('what did I work on this week?');
    await page.keyboard.press('Enter');
    await expect.poll(() => calls.includes('POST message')).toBe(true);
    await expect.poll(() => lastWrite(writes, 'agents.ask-ai')?.status).toBe('completed');
    expect(errors).toEqual([]);
  });

  e2e(`mcp-token: generate completes the tour at ${size.name}`, async ({ page }) => {
    test.skip(mobile, 'web and desktop only');
    const { writes, calls, errors } = await boot(page, size);
    await startFromHub(page, 'agents.mcp-token');
    await page.getByRole('button', { name: /Generate/ }).click();
    await page.getByRole('button', { name: 'Ok', exact: true }).click();
    await expect.poll(() => calls.includes('POST smart-device')).toBe(true);
    await expect.poll(() => lastWrite(writes, 'agents.mcp-token')?.status).toBe('completed');
    expect(errors).toEqual([]);
  });

  e2e(`personas.create: create then the grants spot, completes at ${size.name}`, async ({ page }) => {
    test.skip(mobile, 'web and desktop only');
    const { writes, calls, errors } = await boot(page, size);
    await startFromHub(page, 'personas.create');
    await page.getByPlaceholder('Reviewer').fill('Reviewer');
    await page.getByRole('button', { name: 'Create persona' }).click();
    await expect.poll(() => calls.includes('POST personas')).toBe(true);
    await expect.poll(() => lastWrite(writes, 'personas.create')?.step_id).toBe('s2');
    await expect(popover(page)).toContainText('Workspace access');
    for (const scheme of SCHEMES) await shoot(page, 'personas.create-grants', size, scheme);
    await popover(page).getByRole('button', { name: 'Done' }).click();
    await expect.poll(() => lastWrite(writes, 'personas.create')?.status).toBe('completed');
    expect(errors).toEqual([]);
  });

  e2e(`personas.token: connect an agent completes the tour at ${size.name}`, async ({ page }) => {
    test.skip(mobile, 'web and desktop only');
    const { writes, calls, errors } = await boot(page, size);
    await startFromHub(page, 'personas.token');
    await page.locator('[data-tour="personas.tokens"]').first().click();
    await page.getByRole('button', { name: 'Connect an agent' }).click();
    await shoot(page, 'personas.token-dialog', size, 'light');
    await page.getByRole('dialog', { name: 'Connect an agent' }).getByPlaceholder('Claude Code on my laptop').fill('Claude Code on my laptop');
    await page.getByRole('button', { name: 'Issue token' }).click();
    await expect.poll(() => calls.includes('POST persona-token')).toBe(true);
    await expect.poll(() => lastWrite(writes, 'personas.token')?.status).toBe('completed');
    expect(errors).toEqual([]);
  });

  e2e(`personas.skills: spot completes on Done at ${size.name}`, async ({ page }) => {
    test.skip(mobile, 'web and desktop only');
    const { writes, errors } = await boot(page, size);
    await startFromHub(page, 'personas.skills');
    await popover(page).getByRole('button', { name: 'Done' }).click();
    await expect.poll(() => lastWrite(writes, 'personas.skills')?.status).toBe('completed');
    expect(errors).toEqual([]);
  });

  e2e(`personas.assign: take me there, assignee wait step, then the persona tip at ${size.name}`, async ({ page }) => {
    test.skip(mobile, 'web and desktop only');
    const { writes, errors } = await boot(page, size);
    await startFromHub(page, 'personas.assign');
    await popover(page).getByRole('button', { name: 'Take me there' }).click();
    await expect(page).toHaveURL(/\/demo\/list$/);
    await page.locator('[data-task-id="1"]').first().click();
    await expect(page.locator('[data-tour="task-form.assignees"]').first()).toBeVisible();
    await expect(popover(page)).toContainText('Assign the task');
    for (const scheme of SCHEMES) await shoot(page, 'personas.assign-s1', size, scheme);
    await fire(page, 'persona.assigned', { taskId: 1 });
    await expect.poll(() => lastWrite(writes, 'personas.assign')?.step_id).not.toBe('s1');
    expect(errors).toEqual([]);
  });

  e2e(`ws.modules walks three steps and completes at ${size.name}`, async ({ page }) => {
    const { writes, errors } = await boot(page, size);
    await startFromHub(page, 'ws.modules');
    for (const scheme of SCHEMES) await shoot(page, 'ws.modules-presets', size, scheme);
    await popover(page).getByRole('button', { name: 'Next' }).click();
    await expect(popover(page)).toContainText('Modules');
    for (const scheme of SCHEMES) await shoot(page, 'ws.modules-list', size, scheme);
    await popover(page).getByRole('button', { name: 'Next' }).click();
    await expect(popover(page)).toContainText('Hide for yourself');
    await popover(page).getByRole('button', { name: 'Done' }).click();
    await expect.poll(() => lastWrite(writes, 'ws.modules')?.status).toBe('completed');
    expect(errors).toEqual([]);
  });

  e2e(`ws.invite: send an invitation completes the tour at ${size.name}`, async ({ page }) => {
    const { writes, calls, errors } = await boot(page, size);
    await startFromHub(page, 'ws.invite');
    await page.locator('[data-tour="workspace.members"]').first().click();
    await page.locator('textarea').first().fill('new.member@example.test');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await expect.poll(() => calls.includes('POST invitations')).toBe(true);
    await expect.poll(() => lastWrite(writes, 'ws.invite')?.status).toBe('completed');
    expect(errors).toEqual([]);
  });

  e2e(`ws.dashboard: take me there, spot, member tip at ${size.name}`, async ({ page }) => {
    const { writes, errors } = await boot(page, size);
    await startFromHub(page, 'ws.dashboard');
    await popover(page).getByRole('button', { name: 'Take me there' }).click();
    await expect(page).toHaveURL(/\/demo\/dashboard$/);
    await expect(popover(page)).toContainText('Team activity');
    await expectClear(page, 'dashboard.activity');
    for (const scheme of SCHEMES) await shoot(page, 'ws.dashboard-s1', size, scheme);
    await popover(page).getByRole('button', { name: 'Next' }).click();
    await expect(popover(page)).toContainText('Team members');
    for (const scheme of SCHEMES) await shoot(page, 'ws.dashboard-s2', size, scheme);
    await popover(page).getByRole('button', { name: 'Done' }).click();
    await expect.poll(() => lastWrite(writes, 'ws.dashboard')?.status).toBe('completed');
    expect(errors).toEqual([]);
  });

  for (const id of ['ws.workspaces', 'ws.appearance', 'ws.notifications', 'ws.privacy', 'ws.telegram', 'ws.default-landing']) {
    e2e(`${id} spot completes through the popover at ${size.name}`, async ({ page }) => {
      test.skip(mobile && id === 'ws.telegram', 'web and desktop only');
      const { writes, errors } = await boot(page, size);
      await startFromHub(page, id);
      for (let i = 0; i < 3; i++) {
        const primary = popover(page).getByRole('button', { name: /^(Next|Done)$/ });
        if (!(await primary.count())) break;
        const label = await primary.innerText();
        await primary.click();
        if (label === 'Done') break;
      }
      await expect.poll(() => lastWrite(writes, id)?.status).toBe('completed');
      expect(errors).toEqual([]);
    });
  }

  e2e(`skip tour from the menu on a spot tour marks it skipped at ${size.name}`, async ({ page }) => {
    const { writes, errors } = await boot(page, size);
    await startFromHub(page, 'ws.appearance');
    await page.waitForTimeout(600);
    if (mobile) {
      await popover(page).getByRole('button', { name: 'Skip tour' }).click();
    } else {
      await popover(page).getByRole('button', { name: /^Skip$/ }).click();
      await page.getByRole('menuitem', { name: /Skip tour/ }).click();
    }
    await expect.poll(() => lastWrite(writes, 'ws.appearance')?.status).toBe('skipped');
    expect(errors).toEqual([]);
  });

  e2e(`skip tour on a wait step marks it skipped at ${size.name}`, async ({ page }) => {
    const { writes, errors } = await boot(page, size);
    await startFromHub(page, 'agents.phone-alerts');
    await popover(page).getByRole('button', { name: 'Skip tour' }).click();
    await expect.poll(() => lastWrite(writes, 'agents.phone-alerts')?.status).toBe('skipped');
    expect(errors).toEqual([]);
  });
}

e2e('every S4a anchor is in the DOM on its page, dialog anchors after opening', async ({ page }) => {
  await boot(page, SIZES[0], { runs: [runBody()] });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  await attached(page, ['task-form.agent-work', 'task-form.agent-totals', 'task-form.assignees']);
  await page.goto('/demo/list');
  await attached(page, ['nav.ask-ai', 'nav.notification-bell', 'nav.workspace-switcher', 'nav.menu-button']);
  await page.locator('[data-tour="nav.ask-ai"]').first().click();
  await attached(page, ['ai.input']);
  await page.goto('/settings/agent-notifications');
  await attached(page, ['agent-notify.connect']);
  await page.getByRole('button', { name: 'Create token' }).click();
  await attached(page, ['agent-notify.command', 'agent-notify.test']);
  await page.goto('/settings?tab=device');
  await attached(page, ['settings.device-token']);
  await page.goto('/settings/personas');
  await attached(page, ['personas.create', 'personas.grants', 'personas.tokens', 'personas.skills']);
  await page.goto('/settings/modules');
  await attached(page, ['modules.presets', 'modules.list', 'modules.hide', 'modules.graph', 'settings.default-landing']);
  await page.goto('/settings/workspaces');
  await attached(page, ['workspace.members']);
  await page.locator('[data-tour="workspace.members"]').first().click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.goto('/demo/dashboard');
  await attached(page, ['dashboard.activity', 'dashboard.member']);
  await page.goto('/settings?tab=theme');
  await attached(page, ['settings.theme']);
  await page.goto('/settings?tab=privacy');
  await attached(page, ['settings.privacy-telemetry']);
  await page.goto('/settings?tab=about');
  await attached(page, ['settings.tips']);
  await page.goto('/settings?tab=profile');
  await attached(page, ['settings.telegram']);
  await setScheme(page, 'light');
});

