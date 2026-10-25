import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const E2E = process.env.ONBOARDING_E2E === '1';
const flow = E2E ? test : test.skip;
const SHOTS =
  process.env.ONBOARDING_SHOTS ??
  `${process.env.HOME}/projects/own-projects/tmgr.dev/docs/reports/onboarding/s3`;

const SIZES = [
  { name: 'desktop', width: 1280, height: 800 },
  { name: 'phone', width: 390, height: 844 },
];
const SCHEMES = ['light', 'dark'];
const NEW_USER = { created_at: new Date().toISOString() };
const OLD_USER = { created_at: '2026-01-01T10:00:00Z' };

const setScheme = async (page, scheme) => {
  await page.emulateMedia({ colorScheme: scheme });
  await page.evaluate(
    (dark) => document.documentElement.classList.toggle('dark', dark),
    scheme === 'dark',
  );
  await page.waitForTimeout(300);
};

const shoot = async (page, name, size, scheme) => {
  await setScheme(page, scheme);
  await page.screenshot({
    path: `${SHOTS}/${name}-${size.name}-${scheme}.png`,
  });
};

const popover = (page) =>
  page.locator('[role="dialog"][aria-labelledby^="tour-"]').first();

const baseTask = {
  description: null,
  description_json: null,
  status: 'created',
  status_id: 1,
  workspace_id: 1,
  user_id: 1,
  assignees: [],
  settings: [],
  checkpoints: [],
  common_time: 120,
  start_time: 0,
  approximately_time: 0,
  project_category_id: null,
  created_at: '2026-10-12T10:00:00Z',
  updated_at: '2026-10-12T10:00:00Z',
};

const boot = async (
  page,
  size,
  { onboarding = {}, user = OLD_USER, ...options } = {},
) => {
  await page.setViewportSize({ width: size.width, height: size.height });
  const app = await mockApp(page, {
    multiStatus: true,
    onboarding: { tips_mode: 'auto', ...onboarding },
    userExtra: user,
    ...options,
  });
  await page.route('**/api/tasks/*/countdown', async (route) => {
    const stop = route.request().method() === 'DELETE';
    await route.fulfill({
      json: {
        data: {
          ...baseTask,
          id: 1,
          title: 'Original task',
          start_time: stop ? 0 : 1,
        },
      },
    });
  });
  return app.onboarding;
};

const expectStarters = (page) =>
  page.addInitScript(() =>
    localStorage.setItem('tmgr:onboarding:starters-expected', '3'),
  );

const SIZES_SCHEMES = SIZES.flatMap((size) =>
  SCHEMES.map((scheme) => [size, scheme]),
);

test.describe('hub checklist and sample tasks', () => {
  for (const [size, scheme] of SIZES_SCHEMES) {
    test(`the hub shows the checklist card at ${size.name}, ${scheme}`, async ({
      page,
    }) => {
      await boot(page, size, {
        onboarding: { intent: 'human', facts: { has_task: true } },
      });
      await page.goto('/demo/getting-started');
      await expect(page.locator('[data-tour="hub.checklist"]')).toBeVisible();
      await expect(page.getByTestId('checklist-card')).toBeVisible();
      await expect(page.locator('[data-tour="hub.catalog"]')).toBeVisible();
      await shoot(page, 'hub-checklist', size, scheme);
    });
  }

  for (const size of SIZES) {
    flow(
      `checklist: Hide checklist then Show checklist at ${size.name}`,
      async ({ page }) => {
        const fx = await boot(page, size, {
          onboarding: { intent: 'human', facts: { has_task: true } },
        });
        await page.goto('/demo/getting-started');
        await expect(
          page.locator('[data-checklist-item]').first(),
        ).toBeVisible();
        await expect(
          page.locator('[data-checklist-item][data-done="true"]'),
        ).toHaveCount(1);
        if (size.width < 640) {
          const box = await page.getByTestId('checklist-menu').boundingBox();
          expect(box.height).toBeGreaterThanOrEqual(44);
        }
        await page.getByTestId('checklist-menu').click();
        await page.getByTestId('checklist-hide').click();
        await expect
          .poll(
            () =>
              fx.writes
                .filter((w) => w.path === 'onboarding/preferences')
                .at(-1)?.body,
          )
          .toEqual({ checklist_hidden: true });
        await expect(page.getByTestId('checklist-show')).toBeVisible();
        await expect(page.locator('[data-tour="hub.checklist"]')).toBeVisible();
        await page.getByTestId('checklist-show').click();
        await expect
          .poll(
            () =>
              fx.writes
                .filter((w) => w.path === 'onboarding/preferences')
                .at(-1)?.body,
          )
          .toEqual({ checklist_hidden: false });
        await expect(
          page.getByTestId('checklist-card').getByRole('region'),
        ).toBeVisible();
      },
    );

    flow(
      `agent checklist lists six items and greys the module-off ones at ${size.name}`,
      async ({ page }) => {
        await boot(page, size, {
          onboarding: { intent: 'agent', facts: { mcp_first_call_at: true } },
        });
        await page.goto('/demo/getting-started');
        await expect(page.locator('[data-checklist-item]')).toHaveCount(6);
        await expect(
          page.getByTestId('checklist-card').getByText('Connect an agent'),
        ).toBeVisible();
      },
    );
  }

  for (const [size, scheme] of SIZES_SCHEMES) {
    flow(
      `remove sample tasks from the hub at ${size.name}, ${scheme}`,
      async ({ page }) => {
        const fx = await boot(page, size, {
          onboarding: { intent: 'human', starterTasks: 3 },
        });
        await expectStarters(page);
        await page.goto('/demo/getting-started');
        const button = page.getByTestId('remove-sample-tasks');
        await expect(button).toBeVisible();
        if (size.width < 640)
          expect((await button.boundingBox()).height).toBeGreaterThanOrEqual(
            44,
          );
        await shoot(page, 'remove-samples-hub', size, scheme);
        await button.click();
        await expect(
          page.getByText('Removed 3 sample tasks').first(),
        ).toBeVisible();
        await expect(button).toBeHidden();
        expect(
          fx.writes.some(
            (w) =>
              w.method === 'DELETE' && w.path === 'onboarding/starter-tasks',
          ),
        ).toBe(true);
        await shoot(page, 'remove-samples-done', size, scheme);
        await page.reload();
        await expect(page.getByTestId('remove-sample-tasks')).toBeHidden();
      },
    );
  }

  flow(
    'remove sample tasks is offered in the board toolbar and refetches the board',
    async ({ page }) => {
      const fx = await boot(page, SIZES[0], {
        onboarding: { intent: 'human', starterTasks: 3 },
        taskSettings: [{ key: 'starter', value: '1' }],
      });
      await page.goto('/demo/board');
      const button = page.getByTestId('remove-sample-tasks').first();
      await expect(button).toBeVisible();
      const before = fx.writes.length;
      await button.click();
      await expect(
        page.getByText('Removed 3 sample tasks').first(),
      ).toBeVisible();
      expect(fx.writes.length).toBeGreaterThan(before);
    },
  );

  flow(
    'no remove button on a board without starter tasks',
    async ({ page }) => {
      await boot(page, SIZES[0], {
        onboarding: { intent: 'human', starterTasks: 3 },
      });
      await page.goto('/demo/board');
      await expect(page.getByTestId('remove-sample-tasks')).toHaveCount(0);
    },
  );

  flow(
    'no remove button on the hub after an empty-workspace start',
    async ({ page }) => {
      await boot(page, SIZES[0], { onboarding: { intent: 'human' } });
      await page.goto('/demo/getting-started');
      await expect(page.getByTestId('remove-sample-tasks')).toHaveCount(0);
    },
  );

  flow('removing when nothing is left says so', async ({ page }) => {
    await boot(page, SIZES[0], {
      onboarding: { intent: 'human', starterTasks: 0 },
    });
    await expectStarters(page);
    await page.goto('/demo/getting-started');
    await page.getByTestId('remove-sample-tasks').click();
    await expect(
      page.getByText('No sample tasks to remove').first(),
    ).toBeVisible();
    await expect(page.getByTestId('remove-sample-tasks')).toBeHidden();
  });

  flow('no remove button for intent skipped or null', async ({ page }) => {
    await boot(page, SIZES[0], {
      onboarding: { intent: 'skipped', starterTasks: 3 },
    });
    await page.goto('/demo/getting-started');
    await expect(page.getByTestId('remove-sample-tasks')).toBeHidden();
  });
});

test.describe('new-account flow (needs W1 + W2 code, ONBOARDING_E2E=1)', () => {
  for (const [size, scheme] of SIZES_SCHEMES) {
    flow(
      `human: intent -> G1 -> G2 -> timer aha toast at ${size.name}, ${scheme}`,
      async ({ page }) => {
        const fx = await boot(page, size, { user: NEW_USER });
        await page.goto('/');
        await expect(page).toHaveURL(/\/welcome-setup$/);
        await expect(
          page.getByRole('button', { name: /Just track my tasks/ }),
        ).toBeVisible();
        await expect(
          page.getByRole('button', { name: /Connect my AI agent/ }),
        ).toBeVisible();
        await shoot(page, 'intent', size, scheme);

        await page.getByRole('button', { name: /Just track my tasks/ }).click();
        await expect
          .poll(
            () => fx.writes.find((w) => w.path === 'onboarding/intent')?.body,
          )
          .toMatchObject({ intent: 'human' });
        await expect(page).toHaveURL(/\/demo\/list$/);
        await expect(popover(page)).toBeVisible();
        await shoot(page, 'human-g1', size, scheme);
        await page.keyboard.press('Escape');

        await page.goto('/demo/list');
        await page.locator('[data-task-id="1"]').first().click();
        await page.getByTitle('Start timer').click();
        await expect(page.getByTitle('Stop timer')).toBeVisible();
        await page.getByRole('button', { name: 'Ok', exact: true }).click();
        fx.state.facts.has_timer_minute = true;
        await page.getByTitle('Stop timer').click();
        await expect(
          page
            .getByText(/Logged \d+m on .*Your time stays with the task\./)
            .first(),
        ).toBeVisible();
        await shoot(page, 'human-aha-timer', size, scheme);
        expect(
          fx.factsRequests.some((r) => r.keys.includes('has_timer_minute')),
        ).toBe(true);
      },
    );

    flow(
      `agent: connect -> Connected -> prompt -> aha at ${size.name}, ${scheme}`,
      async ({ page }) => {
        const fx = await boot(page, size, { user: NEW_USER });
        await page.route('**/api/smart-device/token/**', (route) =>
          route.fulfill({
            json: { data: { token: 'fixture-token', has_token: false } },
          }),
        );
        await page.goto('/');
        await page.getByRole('button', { name: /Connect my AI agent/ }).click();
        await expect(page).toHaveURL(/\/welcome-setup\/agent$/);
        await expect(page.getByText('Waiting for your agent')).toBeVisible();
        await shoot(page, 'agent-connect', size, scheme);

        fx.state.facts.mcp_first_call_at = true;
        await page.evaluate(() =>
          document.dispatchEvent(new Event('visibilitychange')),
        );
        await expect(page.getByText('Connected', { exact: true })).toBeVisible({
          timeout: 20000,
        });
        await expect(page.getByTestId('starter-prompt')).toBeVisible();
        await shoot(page, 'agent-connected-prompt', size, scheme);

        fx.state.facts.agent_runs_finished = true;
        await expect(page.getByText('Run logged')).toBeVisible({
          timeout: 40000,
        });
      },
    );

    flow(
      `agent: still waiting after 3 minutes at ${size.name}, ${scheme}`,
      async ({ page }) => {
        await page.clock.install();
        await boot(page, size, { user: NEW_USER });
        await page.route('**/api/smart-device/token/**', (route) =>
          route.fulfill({
            json: { data: { token: 'fixture-token', has_token: false } },
          }),
        );
        await page.goto('/welcome-setup/agent');
        await expect(page.getByTestId('shown-once')).toBeVisible();
        await page.clock.fastForward(181000);
        await expect(
          page.getByRole('heading', { name: 'Still waiting for your agent' }),
        ).toBeVisible();
        await expect(
          page.getByRole('button', { name: 'Check again' }),
        ).toBeVisible();
        await expect(
          page.getByRole('button', { name: 'Do this later' }),
        ).toBeVisible();
        await shoot(page, 'agent-still-waiting', size, scheme);
      },
    );
  }

  flow('skip: no auto tour afterwards', async ({ page }) => {
    const fx = await boot(page, SIZES[0], { user: NEW_USER });
    await page.goto('/');
    await page.getByRole('button', { name: /Skip, I'll look around/ }).click();
    await expect
      .poll(() => fx.writes.find((w) => w.path === 'onboarding/intent')?.body)
      .toMatchObject({ intent: 'skipped' });
    await expect(page).toHaveURL(/\/demo\/list$/);
    await page.waitForTimeout(1500);
    await expect(popover(page)).toHaveCount(0);
    await page.reload();
    await page.waitForTimeout(1500);
    await expect(popover(page)).toHaveCount(0);
  });

  flow('empty workspace sends seed:false', async ({ page }) => {
    const fx = await boot(page, SIZES[0], { user: NEW_USER });
    await page.goto('/');
    await page
      .getByRole('button', { name: /Start with an empty workspace/ })
      .click();
    await expect
      .poll(() => fx.writes.find((w) => w.path === 'onboarding/intent')?.body)
      .toEqual({ intent: 'human', seed: false });
  });

  flow('invited user is not redirected', async ({ page }) => {
    const fx = await boot(page, SIZES[0], {
      user: NEW_USER,
      onboarding: { invited: true },
    });
    await page.goto('/');
    await expect(page).toHaveURL(/\/demo\/list$/);
    expect(fx.writes.some((w) => w.path === 'onboarding/intent')).toBe(false);
  });

  flow('user with a pending invitation is not redirected', async ({ page }) => {
    await page.addInitScript(() =>
      localStorage.setItem('workspace.invitation', 'pending-fixture'),
    );
    await boot(page, SIZES[0], { user: NEW_USER });
    await page.goto('/');
    await expect(page).not.toHaveURL(/welcome-setup/);
  });

  flow(
    'existing user with intent null and an old created_at is not redirected',
    async ({ page }) => {
      await boot(page, SIZES[0], { user: OLD_USER });
      await page.goto('/');
      await expect(page).toHaveURL(/\/demo\/list$/);
      await page.waitForTimeout(1000);
      await expect(page).not.toHaveURL(/welcome-setup/);
    },
  );
});

test.describe('welcome intent cards', () => {
  flow('the two intent cards have the same height at 390', async ({ page }) => {
    await boot(page, SIZES[1], { user: NEW_USER });
    await page.goto('/');
    await expect(page).toHaveURL(/\/welcome-setup$/);
    const human = page.getByRole('button', { name: /Just track my tasks/ });
    const agent = page.getByRole('button', { name: /Connect my AI agent/ });
    await expect(agent).toBeVisible();
    const [a, b] = [await human.boundingBox(), await agent.boundingBox()];
    expect(Math.abs(a.height - b.height)).toBeLessThan(1);
    await shoot(page, 'intent-cards', SIZES[1], 'light');
  });
});

test.describe('welcome screens: consent banner and keyboard', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  flow('the consent banner never hides Skip at 390', async ({ page }) => {
    await boot(page, SIZES[1], { user: NEW_USER, consent: false });
    await page.goto('/');
    await expect(page).toHaveURL(/\/welcome-setup$/);
    const banner = page.locator('[data-consent-panel]');
    await expect(banner).toBeVisible();
    const skip = page.getByTestId('intent-skip');
    await page
      .getByTestId('welcome-setup')
      .evaluate((el) => el.scrollTo(0, el.scrollHeight));
    const skipBox = await skip.boundingBox();
    const bannerBox = await banner.boundingBox();
    expect(skipBox.y + skipBox.height).toBeLessThanOrEqual(bannerBox.y);
    expect(skipBox.height).toBeGreaterThanOrEqual(44);
    await shoot(page, 'intent-consent', SIZES[1], 'light');
    await page.getByRole('button', { name: 'Decline' }).click();
  });

  flow(
    'keyboard reaches the intent cards first and the shell is inert',
    async ({ page }) => {
      await boot(page, SIZES[0], { user: NEW_USER });
      await page.goto('/');
      await expect(page).toHaveURL(/\/welcome-setup$/);
      await page.keyboard.press('Tab');
      await expect(
        page.getByRole('button', { name: /Just track my tasks/ }),
      ).toBeFocused();
      await page.keyboard.press('Tab');
      await expect(
        page.getByRole('button', { name: /Connect my AI agent/ }),
      ).toBeFocused();
    },
  );

  flow(
    'connect screen: tabs, copy and status are keyboard reachable',
    async ({ page }) => {
      await boot(page, SIZES[1], { user: NEW_USER });
      await page.route('**/api/smart-device/token/**', (route) =>
        route.fulfill({
          json: { data: { token: 'fixture-token', has_token: false } },
        }),
      );
      await page.goto('/welcome-setup/agent');
      const tabs = page.getByRole('tab');
      await expect(tabs).toHaveCount(4);
      for (const tab of await tabs.all())
        expect((await tab.boundingBox()).height).toBeGreaterThanOrEqual(44);
      await page.getByRole('tab', { name: 'Claude Code' }).focus();
      await page.keyboard.press('ArrowRight');
      await expect(page.getByRole('tab', { name: 'Codex' })).toHaveAttribute(
        'aria-selected',
        'true',
      );
      await page.keyboard.press('Tab');
      await expect(page.getByRole('button', { name: 'Copy' })).toBeFocused();
      const copy = await page
        .getByRole('button', { name: 'Copy' })
        .boundingBox();
      expect(copy.height).toBeGreaterThanOrEqual(44);
      await expect(
        page.getByRole('status').filter({ hasText: 'Waiting for your agent' }),
      ).toHaveAttribute('aria-live', 'polite');
    },
  );
});

const mockModules = async (page, { failPut = false } = {}) => {
  const fx = { puts: [], on: { agent_work: false, personas: false } };
  await page.route('**/api/workspaces/1/modules', async (route) => {
    const request = route.request();
    if (request.method() === 'PUT') {
      fx.puts.push(request.postDataJSON());
      if (failPut) {
        await route.fulfill({
          status: 500,
          json: { message: 'Fixture failure' },
        });
        return;
      }
      Object.assign(fx.on, request.postDataJSON().modules);
    }
    const configured = fx.puts.length > 0 && !failPut;
    await route.fulfill({
      json: {
        data: {
          configured,
          preset: configured ? 'custom' : null,
          can_manage: true,
          packs: [{ key: 'ai', name: 'AI agents' }],
          presets: [],
          notices: [],
          modules: Object.entries(fx.on).map(([key, enabled]) => ({
            key,
            name: key === 'personas' ? 'Personas' : 'Agent work',
            pack: 'ai',
            scope: 'workspace',
            core: false,
            enabled,
            hidden: false,
          })),
        },
      },
    });
  });
  return fx;
};

const picker = (page) =>
  page.getByRole('dialog', { name: 'What do you want TMGR for?' });

const mockConnectToken = (page) =>
  page.route('**/api/smart-device/token/**', (route) =>
    route.fulfill({
      json: { data: { token: 'fixture-token', has_token: false } },
    }),
  );

test.describe('agent intent turns on agent work and personas (TM-526)', () => {
  for (const [size, scheme] of SIZES_SCHEMES) {
    flow(
      `agent intent enables the agent modules at ${size.name}, ${scheme}`,
      async ({ page }) => {
        await boot(page, size, { user: NEW_USER });
        const modules = await mockModules(page);
        await mockConnectToken(page);
        await page.goto('/');
        await expect(page).toHaveURL(/\/welcome-setup$/);
        await page.getByRole('button', { name: /Connect my AI agent/ }).click();
        await expect(page).toHaveURL(/\/welcome-setup\/agent$/);
        expect(modules.puts).toEqual([
          { modules: { agent_work: true, personas: true } },
        ]);
        await shoot(page, 'tm526-agent-connect', size, scheme);

        await page.goto('/demo/getting-started');
        const run = page.locator('[data-checklist-item="first-agent-run"]');
        await expect(run).toBeVisible();
        await expect(run.locator('em')).toHaveCount(0);
        await shoot(page, 'tm526-agent-checklist', size, scheme);
        await expect(picker(page)).toHaveCount(0);

        await page.goto('/settings/personas');
        await expect(
          size.width < 640
            ? page.getByRole('heading', { name: 'Personas', level: 1 })
            : page.getByRole('link', { name: 'Personas' }).first(),
        ).toBeVisible();
        await shoot(page, 'tm526-agent-personas', size, scheme);
        expect(modules.puts).toHaveLength(1);
      },
    );
  }

  flow(
    'human intent leaves the modules alone and gets the picker after',
    async ({ page }) => {
      const fx = await boot(page, SIZES[0], { user: NEW_USER });
      const modules = await mockModules(page);
      await page.goto('/');
      await expect(picker(page)).toHaveCount(0);
      await page.getByRole('button', { name: /Just track my tasks/ }).click();
      await expect(page).toHaveURL(/\/demo\/list$/);
      await expect
        .poll(() => fx.writes.find((w) => w.path === 'onboarding/intent')?.body)
        .toMatchObject({ intent: 'human' });
      await expect(picker(page)).toBeVisible();
      await shoot(page, 'tm526-human-picker', SIZES[0], 'light');
      expect(modules.puts).toEqual([]);
    },
  );

  flow('skip leaves the modules alone', async ({ page }) => {
    await boot(page, SIZES[0], { user: NEW_USER });
    const modules = await mockModules(page);
    await page.goto('/');
    await page.getByTestId('intent-skip').click();
    await expect(page).toHaveURL(/\/demo\/list$/);
    await expect(picker(page)).toBeVisible();
    expect(modules.puts).toEqual([]);
  });

  flow(
    'a failed modules save still opens the connect screen',
    async ({ page }) => {
      await boot(page, SIZES[0], { user: NEW_USER });
      const modules = await mockModules(page, { failPut: true });
      await mockConnectToken(page);
      await page.goto('/');
      await page.getByRole('button', { name: /Connect my AI agent/ }).click();
      await expect(page).toHaveURL(/\/welcome-setup\/agent$/);
      await expect(page.getByText('Waiting for your agent')).toBeVisible();
      await expect(picker(page)).toHaveCount(0);
      expect(modules.puts).toHaveLength(1);

      await page.goto('/demo/getting-started');
      await expect(
        page.locator('[data-checklist-item="first-agent-run"] em'),
      ).toBeVisible();
      await expect(picker(page)).toBeVisible();
      await picker(page).getByRole('button', { name: 'Not now' }).click();
      await expect(picker(page)).toHaveCount(0);
      await page.goto('/settings/personas');
      await expect(
        page.getByRole('heading', { name: 'Personas', level: 1 }),
      ).toBeVisible();
      await expect(page.getByRole('link', { name: 'Personas' })).toHaveCount(0);
    },
  );
});
