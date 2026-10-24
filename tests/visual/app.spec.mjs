import { expect, test } from '@playwright/test';
import { withData } from './fixtures.mjs';
import { boot, shoot } from './visualApp.mjs';

// TM-209: pixel baseline of the app (light + dark, desktop + mobile) to prove the Tailwind 4 migration
// changes no rendering. See playwright.visual.config.mjs for how to capture and compare.

// The prod build keeps app.__vue_app__ on the mount element.
const push = (page, path) =>
  page.evaluate(
    (to) =>
      document
        .querySelector('#app')
        .__vue_app__.config.globalProperties.$router.push(to),
    path,
  );
const openCreateTask = (page) =>
  page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
const listReady = (page) => page.locator('[data-task-id="1"]:visible').first();

const settings = (tab) => ({
  name: `settings-${tab}`,
  url: `/settings?tab=${tab}`,
  inApp: true,
  ready: (page) => page.getByRole('main').first(),
});

// name, url, guest, empty, open(page) after goto, ready(page) -> locator to wait for,
// mask(page): locators painted over (truly dynamic bits only),
// inApp: boot on /demo/list, then router.push(url),
// overlay: viewport-only shot (fixed backdrops don't span a full-page capture),
// only: 'desktop' | 'mobile', dracula / webkit: also shot in that theme / browser.
const SCENARIOS = [
  {
    name: 'login',
    guest: true,
    url: '/login',
    ready: (p) => p.getByRole('button', { name: 'Sign in' }),
    dracula: true,
    webkit: true,
  },
  {
    name: 'login-error',
    guest: true,
    url: '/login',
    open: async (page) => {
      await page.route('**/api/auth/login', (r) =>
        r.fulfill({
          status: 401,
          json: { message: 'Invalid email or password' },
        }),
      );
      await page.getByPlaceholder('you@company.com').fill('test@example.test');
      await page.getByPlaceholder('Password').fill('wrong-password');
      await page.getByRole('button', { name: 'Sign in' }).click();
    },
    ready: (p) => p.getByText('Invalid email or password').first(),
    webkit: true,
  },
  {
    name: 'register',
    guest: true,
    url: '/register',
    ready: (p) => p.getByRole('button', { name: 'Create account' }),
    webkit: true,
  },
  {
    name: 'password-forget',
    guest: true,
    url: '/password/forget',
    ready: (p) => p.getByRole('button', { name: 'Send reset link' }),
    webkit: true,
  },
  {
    name: 'password-reset',
    guest: true,
    url: '/password/reset?token=fixture&email=test%40example.test',
    ready: (p) => p.getByRole('button').first(),
  },
  {
    name: 'dashboard',
    url: '/demo/dashboard',
    ready: (p) => p.locator('[data-kpi="activity"]'),
    dracula: true,
    webkit: true,
  },
  {
    name: 'board',
    url: '/demo/board',
    ready: (p) => p.getByText('Refactor the board drag and drop').first(),
    dracula: true,
  },
  {
    name: 'board-empty',
    url: '/demo/board',
    empty: true,
    ready: (p) => p.getByText('Backlog').first(),
  },
  { name: 'list', url: '/demo/list', ready: listReady },
  {
    name: 'list-empty',
    url: '/demo/list',
    empty: true,
    ready: (p) => p.getByRole('main').first(),
  },
  {
    name: 'task-modal',
    overlay: true,
    // On phones a tap opens the task page (task-page covers it).
    only: 'desktop',
    url: '/demo/list',
    open: async (page) => {
      await listReady(page).click();
    },
    ready: (p) => p.locator('.new-form-container').first(),
    dracula: true,
  },
  {
    name: 'task-page',
    url: '/demo/tasks/1',
    ready: (p) => p.getByText('Can we split this into two PRs?').first(),
  },
  {
    name: 'task-create',
    overlay: true,
    url: '/demo/list',
    open: async (page) => {
      await listReady(page).waitFor();
      await openCreateTask(page);
    },
    ready: (p) => p.locator('.new-form-container').first(),
  },
  {
    name: 'pages-index',
    url: '/demo/pages',
    ready: (p) => p.getByTestId('pages-index-tree'),
  },
  {
    name: 'pages-empty',
    url: '/demo/pages',
    empty: true,
    ready: (p) => p.getByText('No pages yet'),
  },
  {
    name: 'page-view',
    url: '/demo/pages/release-checklist',
    ready: (p) => p.getByText('Never tag on a Friday.'),
  },
  {
    name: 'page-editor-selection',
    url: '/demo/pages/release-checklist',
    open: async (page) => {
      await page.getByText('Never tag on a Friday.').click({ clickCount: 3 });
    },
    // The hover block handle (+ / drag grip) lands at a slightly different spot run to run.
    mask: (p) => [p.locator('.milkdown-block-handle')],
    ready: (p) => p.getByText('Never tag on a Friday.'),
  },
  {
    name: 'routines',
    url: '/routines',
    inApp: true,
    ready: (p) => p.getByText('Inbox zero').first(),
  },
  {
    name: 'category-settings',
    url: '/demo/categories/10',
    ready: (p) => p.getByText('Delete category').first(),
  },
  {
    name: 'category-tasks',
    url: '/demo/categories/10',
    open: async (page) => {
      await page.getByRole('main').getByText('Tasks', { exact: true }).click();
    },
    ready: (p) => p.getByText('Fix login redirect on expired session').first(),
  },
  {
    name: 'notifications',
    url: '/demo/notifications',
    ready: (p) => p.getByRole('main').locator('.notification-item').first(),
  },
  {
    name: 'notifications-empty',
    url: '/demo/notifications',
    empty: true,
    ready: (p) => p.getByText('No notifications').first(),
  },
  {
    name: 'members-modal',
    overlay: true,
    url: '/demo/board',
    only: 'desktop',
    open: async (page) => {
      await page
        .locator('button:has(span[title="Ann Lee"]):visible')
        .first()
        .click();
    },
    ready: (p) => p.getByRole('dialog', { name: 'Workspace Members' }),
  },
  {
    name: 'command-palette',
    overlay: true,
    url: '/demo/list',
    open: async (page) => {
      await listReady(page).waitFor();
      await page.keyboard.press('ControlOrMeta+k');
    },
    ready: (p) => p.getByTestId('command-palette'),
  },
  {
    name: 'dropdown-task-actions',
    overlay: true,
    url: '/demo/list',
    open: async (page) => {
      await listReady(page)
        .getByRole('button', { name: 'Task actions', exact: true })
        .click();
    },
    ready: (p) => p.getByRole('menu'),
  },
  {
    name: 'popover-notifications',
    overlay: true,
    url: '/demo/list',
    only: 'desktop',
    open: async (page) => {
      await listReady(page).waitFor();
      await page.getByRole('button', { name: 'Notifications' }).first().click();
    },
    ready: (p) =>
      p.locator('.notification-dropdown .notification-item').first(),
  },
  {
    name: 'dialog-new-workspace',
    overlay: true,
    url: '/settings/workspaces',
    inApp: true,
    open: async (page) => {
      await page.getByRole('button', { name: 'New workspace' }).first().click();
    },
    ready: (p) => p.getByRole('dialog'),
  },
  {
    name: 'toast-profile-saved',
    overlay: true,
    url: '/profile',
    inApp: true,
    open: async (page) => {
      await page
        .getByRole('button', { name: 'Save', exact: true })
        .first()
        .click();
    },
    ready: (p) => p.getByText('User data saved').first(),
  },
  {
    name: 'settings-sections',
    url: '/settings/sections',
    inApp: true,
    only: 'mobile',
    ready: (p) => p.getByText('Profile').first(),
  },
  {
    name: 'settings-workspaces',
    url: '/settings/workspaces',
    inApp: true,
    ready: (p) => p.getByText('Default workspace').first(),
  },
  settings('profile'),
  settings('notification'),
  settings('device'),
  settings('privacy'),
  settings('theme'),
  settings('agent-work'),
  settings('about'),
  {
    name: 'settings-modules',
    url: '/settings/modules',
    inApp: true,
    ready: (p) => p.getByRole('main').first(),
  },
  {
    name: 'settings-personas',
    url: '/settings/personas',
    inApp: true,
    ready: (p) => p.getByRole('main').first(),
  },
  {
    name: 'settings-agent-notifications',
    url: '/settings/agent-notifications',
    inApp: true,
    ready: (p) => p.getByRole('main').first(),
  },
  {
    name: 'settings-plugins',
    url: '/settings/plugins',
    inApp: true,
    ready: (p) => p.getByRole('main').first(),
  },
];

for (const s of SCENARIOS) {
  const schemes = ['light', 'dark', ...(s.dracula ? ['dracula'] : [])];
  for (const scheme of schemes) {
    const tag = s.webkit && scheme !== 'dracula' ? ' @webkit' : '';
    test(`${s.name} ${scheme}${tag}`, async ({ page }, info) => {
      const project = info.project.name;
      test.skip(
        !!s.only && !project.startsWith(s.only),
        `${s.name} is ${s.only}-only`,
      );
      await boot(page, scheme, s.guest ? { account: false } : {});
      if (!s.guest) await withData(page, { empty: !!s.empty });
      if (s.inApp) {
        // Direct loads of module-gated routes (routines, personas, alerts...) on the prod build render
        // "module is off" / "could not load" because the request goes out before the workspace modules
        // are known; open them in-app like a user does.
        await page.goto('/demo/list');
        await listReady(page).waitFor();
        await push(page, s.url);
      } else await page.goto(s.url);
      if (s.open) await s.open(page);
      await expect(s.ready(page)).toBeVisible({ timeout: 30_000 });
      await shoot(page, `${s.name}-${scheme}`, {
        fullPage: !s.overlay,
        mask: s.mask?.(page) ?? [],
      });
    });
  }
}
