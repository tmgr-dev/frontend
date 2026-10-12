import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';
import { SCHEMES, SIZES, mockOnboarding, setScheme, shoot } from './toursHelpers.mjs';

const e2e = process.env.S4_E2E === '1' ? test : test.skip;

const MODULES = [
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

const run = {
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
};

const persona = {
  id: 'uuid-1',
  name: 'Reviewer',
  description: null,
  avatar_url: null,
  system_prompt: '',
  prompt_version: 1,
  archived_at: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
  owner: { id: 1, name: 'Test User' },
};

const mockS4a = async (page, size, onboarding = {}) => {
  await page.setViewportSize({ width: size.width, height: size.height });
  await mockApp(page, { multiStatus: true });
  const fixture = await mockOnboarding(page, { tips_mode: 'hub_only', ...onboarding });
  const calls = [];
  await page.route('**/api/workspaces/1/modules', (route) =>
    route.fulfill({
      json: {
        data: {
          configured: true,
          can_manage: true,
          modules: MODULES.map((key) => ({ key, name: key, enabled: true })),
        },
      },
    }),
  );
  await page.route('**/api/tasks/1/agent-work', (route) =>
    route.fulfill({
      json: {
        data: {
          runs: [run],
          totals: { agent_seconds: 30, human_seconds: 0, human_timer_running: false },
        },
      },
    }),
  );
  await page.route('**/api/notify-tokens*', (route) => {
    if (route.request().method() === 'POST') {
      calls.push('POST notify-tokens');
      return route.fulfill({
        json: { data: { id: 2, label: 'Claude Code on my laptop', prefix: 'tmgrn_cd34', token: 'tmgrn_secret', created_at: '2026-01-01T00:00:00Z' } },
      });
    }
    return route.fulfill({ json: { data: [] } });
  });
  await page.route('**/api/personas*', (route) => {
    if (route.request().method() === 'POST') {
      calls.push('POST personas');
      return route.fulfill({ json: { data: persona } });
    }
    return route.fulfill({ json: { data: [] } });
  });
  await page.route('**/api/workspaces/1/assignable-personas*', (route) =>
    route.fulfill({ json: { data: [persona] } }),
  );
  await page.route('**/api/workspaces/1/invitations*', (route) => {
    if (route.request().method() === 'POST') {
      calls.push('POST invitations');
      return route.fulfill({ json: { data: [] } });
    }
    return route.fulfill({ json: { data: [] } });
  });
  await page.route('**/api/smart-device/**', (route) =>
    route.fulfill({ json: { data: { token: 'device-token' } } }),
  );
  return { ...fixture, calls };
};

const popover = (page) => page.locator('[role="dialog"][aria-labelledby^="tour-"]').first();

const lastWrite = (writes, id) => writes.filter((w) => w.path === `onboarding/tours/${id}`).at(-1)?.body;

const expectAnchors = async (page, anchors) => {
  for (const anchor of anchors) {
    await expect(page.locator(`[data-tour="${anchor}"]`).first(), anchor).toBeAttached();
  }
};

const startFromHub = async (page, id) => {
  await page.goto('/demo/getting-started');
  await page.getByTestId(`hub-start-${id}`).click();
  await expect(popover(page)).toBeVisible();
};

const groups = [
  {
    name: 'agents',
    first: 'agents.work-log',
    notOnMobile: ['agents.mcp-token'],
    driven: 'agents.phone-alerts',
    anchorsByPage: async (page, size) => {
      await page.goto('/demo/list');
      await page.locator('[data-task-id="1"]').first().click();
      await expectAnchors(page, ['task-form.agent-work', 'task-form.agent-totals']);
      await page.goto('/settings/agent-notifications');
      await expectAnchors(page, ['agent-notify.connect']);
      await page.goto('/settings?tab=device');
      await expectAnchors(page, ['settings.device-token']);
      await page.goto('/demo/list');
      await expectAnchors(page, ['nav.ask-ai']);
      await page.locator('[data-tour="nav.ask-ai"]').first().click();
      await expectAnchors(page, ['ai.input']);
    },
    drive: async (page, calls) => {
      await page.goto('/settings/agent-notifications');
      await page.getByPlaceholder('Claude Code on my laptop').fill('Claude Code on my laptop');
      await page.locator('[data-tour="agent-notify.connect"]').first().click();
      await expect.poll(() => calls.includes('POST notify-tokens')).toBe(true);
    },
  },
  {
    name: 'personas',
    first: 'personas.create',
    notOnMobile: ['personas.create', 'personas.token', 'personas.assign', 'personas.skills'],
    driven: 'personas.create',
    anchorsByPage: async (page) => {
      await page.goto('/settings/personas');
      await expectAnchors(page, ['personas.create', 'personas.grants', 'personas.tokens', 'personas.skills']);
    },
    drive: async (page, calls) => {
      await page.goto('/settings/personas');
      await page.getByPlaceholder('Reviewer').fill('Reviewer');
      await page.locator('[data-tour="personas.create"]').first().click();
      await expect.poll(() => calls.includes('POST personas')).toBe(true);
    },
  },
  {
    name: 'workspace',
    first: 'ws.modules',
    notOnMobile: ['ws.telegram'],
    driven: 'ws.invite',
    anchorsByPage: async (page) => {
      await page.goto('/settings/modules');
      await expectAnchors(page, ['modules.list']);
      await page.goto('/settings/workspaces');
      await expectAnchors(page, ['workspace.members']);
      await page.goto('/demo/dashboard');
      await expectAnchors(page, ['dashboard.activity', 'nav.notification-bell']);
      await page.goto('/settings?tab=theme');
      await expectAnchors(page, ['settings.theme']);
      await page.goto('/settings?tab=about');
      await expectAnchors(page, ['settings.privacy-telemetry', 'settings.tips']);
      await page.goto('/settings?tab=profile');
      await expectAnchors(page, ['settings.telegram']);
    },
    drive: async (page, calls) => {
      await page.goto('/settings/workspaces');
      await page.getByRole('button', { name: /Invite/ }).first().click();
      await page.locator('textarea').first().fill('new.member@example.test');
      await page.getByRole('button', { name: 'Send', exact: true }).click();
      await expect.poll(() => calls.includes('POST invitations')).toBe(true);
    },
  },
];

for (const group of groups) {
  for (const size of SIZES) {
    for (const scheme of SCHEMES) {
      e2e(`${group.name}: hub, anchors and one wait step at ${size.name}, ${scheme}`, async ({ page }) => {
        const mobile = size.width < 640;
        await page.emulateMedia({ colorScheme: scheme });
        const { writes, calls } = await mockS4a(page, size);

        await page.goto('/demo/getting-started');
        await setScheme(page, scheme);
        for (const id of group.notOnMobile) {
          if (mobile) await expect(page.getByTestId(`hub-start-${id}`), `unavailable: platform ${id}`).toHaveCount(0);
        }

        const available = !(mobile && group.notOnMobile.includes(group.first));
        if (available) {
          await startFromHub(page, group.first);
          await shoot(page, `${group.name}-${group.first}`, size, scheme);
          await page.keyboard.press('Escape');
          await expect(popover(page)).toBeHidden();
        }

        await group.anchorsByPage(page, size);

        const driveable = !(mobile && group.notOnMobile.includes(group.driven));
        if (driveable) {
          await page.goto('/demo/getting-started');
          await page.getByTestId(`hub-start-${group.driven}`).click();
          await expect(popover(page)).toBeVisible();
          await group.drive(page, calls);
          await expect
            .poll(() => lastWrite(writes, group.driven)?.status)
            .toMatch(/in_progress|completed/);
          await expect
            .poll(() => {
              const body = lastWrite(writes, group.driven);
              return body?.status === 'completed' || body?.step_id !== 's1';
            })
            .toBe(true);
          await shoot(page, `${group.name}-${group.driven}-advanced`, size, scheme);
        }
      });
    }
  }
}
