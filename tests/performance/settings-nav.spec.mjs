import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

const DESKTOP = { width: 1280, height: 800 };
const PHONE = { width: 390, height: 844 };

const quietTours = (page) =>
  page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
  });

const pageRow = (id, title, slug, pinned = false) => ({
  id,
  title,
  slug,
  type: 'plain',
  parent_id: null,
  position: id,
  pinned,
  updated_at: '2026-10-01T10:00:00Z',
});

async function webApp(
  page,
  { off = [], pages = [], hubHidden = false, pagesOn = true } = {},
) {
  await quietTours(page);
  const fixture = await mockApp(page, {
    onboarding: { hub_hidden: hubHidden },
  });
  // A module without an entry counts as enabled, so "off" needs explicit rows.
  await page.route('**/api/workspaces/*/modules', (route) =>
    route.fulfill({
      json: {
        data: {
          configured: true,
          modules: [
            { key: 'pages', enabled: pagesOn },
            ...off.map((key) => ({ key, enabled: false })),
          ],
        },
      },
    }),
  );
  await page.route('**/api/pages**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (/\/pages(\/tree)?$/.test(path))
      return route.fulfill({ json: { data: pages } });
    return route.fulfill({ json: { data: [] } });
  });
  return fixture;
}

const settingsNav = (page) =>
  page.getByRole('navigation', { name: 'Settings' });
const heading = (page) => page.locator('[data-page-header] h1').first();
const sidebar = (page) => page.locator('[data-sidebar="sidebar"]').first();

const openUserMenu = (page, scope = page) =>
  scope.locator('[data-sidebar="footer"] [aria-haspopup="menu"]').click();

const groupItems = (page, group) =>
  settingsNav(page).locator(`ul[aria-labelledby="settings-nav-${group}"] a`);

async function expectActive(page, key) {
  await expect(page.getByTestId(`settings-nav-${key}`)).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(settingsNav(page).locator('[aria-current="page"]')).toHaveCount(
    1,
  );
}

test.describe('desktop web', () => {
  test.use({ viewport: DESKTOP });

  test('the avatar menu holds only Profile, Settings, the theme toggle and Log out', async ({
    page,
  }) => {
    await webApp(page);
    await page.goto('/demo/board');
    await openUserMenu(page);
    const menu = page.getByRole('menu');
    await expect(menu.getByText('test@example.test')).toBeVisible();
    await expect(page.getByRole('menuitem')).toHaveText([
      'Profile',
      'Settings',
      'Dark mode',
      'Log out',
    ]);
  });

  test('with the help hub hidden, Getting started moves from the sidebar to the avatar menu', async ({
    page,
  }) => {
    await webApp(page, { hubHidden: true });
    await page.goto('/demo/board');
    await expect(
      sidebar(page).getByRole('link', { name: 'Getting started' }),
    ).toHaveCount(0);
    await openUserMenu(page);
    await expect(page.getByRole('menuitem')).toHaveText([
      'Profile',
      'Settings',
      'Getting started',
      'Dark mode',
      'Log out',
    ]);
  });

  // D-014: the /settings/sections redirect must not replace the page the user
  // opened Settings from.
  test('Back from Settings opened via the avatar menu returns to the board', async ({
    page,
  }) => {
    await webApp(page);
    await page.goto('/demo/board');
    await expect(
      sidebar(page).getByRole('link', { name: 'Board', exact: true }),
    ).toBeVisible();
    await openUserMenu(page);
    await page.getByRole('menuitem', { name: 'Settings' }).click();
    await expect(page).toHaveURL(/\/settings\/workspaces$/);
    await expect(heading(page)).toHaveText('Workspace settings');
    await page.goBack();
    await expect(page).toHaveURL(/\/demo\/board$/);
  });

  test('Settings opens the settings nav; one item per group opens its page and moves the highlight', async ({
    page,
  }) => {
    await webApp(page);
    await page.goto('/demo/board');
    await openUserMenu(page);
    await page.getByRole('menuitem', { name: 'Settings' }).click();

    await expect(page).toHaveURL(/\/settings\/workspaces$/);
    await expect(heading(page)).toHaveText('Workspace settings');
    await expect(page.getByTestId('settings-layout')).toBeVisible();
    await expectActive(page, 'workspaces');

    await expect(groupItems(page, 'account')).toHaveText([
      'Profile',
      'Notifications',
      'Smart Devices',
      'Privacy',
    ]);
    await expect(groupItems(page, 'workspace')).toHaveText([
      'General',
      'Modules',
      'Personas',
    ]);
    await expect(groupItems(page, 'agents')).toHaveText([
      'Agent work',
      'Agent alerts',
    ]);
    await expect(groupItems(page, 'app')).toHaveText(['Theme', 'About']);

    const steps = [
      ['privacy', /\/settings\?tab=privacy$/, 'Privacy'],
      ['modules', /\/settings\/modules$/, 'Modules'],
      [
        'agent-notifications',
        /\/settings\/agent-notifications$/,
        'Agent alerts',
      ],
      ['agent-work', /\/settings\?tab=agent-work$/, 'Agent work'],
      ['theme', /\/settings\?tab=theme$/, 'Theme'],
      ['profile', /\/settings\?tab=profile$/, 'Profile'],
    ];
    for (const [key, url, title] of steps) {
      await page.getByTestId(`settings-nav-${key}`).click();
      await expect(page).toHaveURL(url);
      await expect(heading(page)).toHaveText(title);
      await expectActive(page, key);
    }
  });

  test('deep links open their section with the right item highlighted', async ({
    page,
  }) => {
    await webApp(page);
    const cases = [
      [
        '/settings?tab=privacy',
        /\/settings\?tab=privacy$/,
        'privacy',
        'Privacy',
      ],
      ['/settings/personas', /\/settings\/personas$/, 'personas', 'Personas'],
      [
        '/settings',
        /\/settings\/workspaces$/,
        'workspaces',
        'Workspace settings',
      ],
      [
        '/settings?tab=workspace',
        /\/settings\/workspaces$/,
        'workspaces',
        'Workspace settings',
      ],
      ['/settings/features', /\/settings\/modules$/, 'modules', 'Modules'],
      [
        '/settings/sections',
        /\/settings\/workspaces$/,
        'workspaces',
        'Workspace settings',
      ],
    ];
    for (const [from, url, key, title] of cases) {
      await page.goto(from);
      await expect(page).toHaveURL(url);
      await expect(heading(page)).toHaveText(title);
      await expectActive(page, key);
    }
  });

  test('a deep link to a hidden section still renders the settings nav without errors', async ({
    page,
  }) => {
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await webApp(page);
    await page.goto('/settings?tab=experimental');
    await expect(heading(page)).toBeVisible();
    await expect(settingsNav(page)).toBeVisible();
    await expect(page.getByTestId('settings-nav-experimental')).toHaveCount(0);
    await expect(
      settingsNav(page).locator('[aria-current="page"]'),
    ).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test('with optional modules off the settings nav and the sidebar drop only those items', async ({
    page,
  }) => {
    await webApp(page, {
      off: ['personas', 'alerts', 'ai.assistant', 'graph'],
    });
    await page.goto('/settings?tab=privacy');
    await expect(heading(page)).toHaveText('Privacy');
    await expect(groupItems(page, 'workspace')).toHaveText([
      'General',
      'Modules',
    ]);
    await expect(groupItems(page, 'agents')).toHaveText(['Agent work']);
    await expect(groupItems(page, 'account')).toHaveCount(4);
    await expect(groupItems(page, 'app')).toHaveText(['Theme', 'About']);

    await expect(page.getByRole('button', { name: 'Ask AI' })).toHaveCount(0);
    await expect(page.locator('[data-tour="nav.ask-ai"]')).toHaveCount(0);
    await expect(
      sidebar(page).getByRole('link', { name: 'Map', exact: true }),
    ).toHaveCount(0);
    await expect(
      page.getByRole('button', { name: 'New task', exact: true }),
    ).toBeVisible();
  });

  test('the settings nav works from the keyboard', async ({ page }) => {
    await webApp(page);
    await page.goto('/settings?tab=privacy');
    await expectActive(page, 'privacy');
    await page.getByTestId('settings-nav-privacy').focus();
    await page.keyboard.press('Tab');
    await expect(page.getByTestId('settings-nav-workspaces')).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await page.keyboard.press('Shift+Tab');
    await expect(page.getByTestId('settings-nav-device')).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(page).toHaveURL(/\/settings\?tab=device$/);
    await expect(heading(page)).toHaveText('Smart devices');
    await expectActive(page, 'device');
  });

  test('the sidebar keeps every nav item and search; New task and Ask AI sit in the header', async ({
    page,
  }) => {
    await webApp(page);
    await page.goto('/demo/board');
    const bar = sidebar(page);
    await expect(
      bar.getByRole('link', { name: 'Board', exact: true }),
    ).toBeVisible();

    await expect(bar.locator('[data-sidebar="group-label"]')).toHaveText([
      'Main',
      'Categories',
      'Pages',
      'Tools',
    ]);
    await expect(page.getByTestId('sidebar-pinned')).toHaveCount(0);

    for (const name of [
      'Dashboard',
      'List',
      'Board',
      'Daily Routines',
      'Archive',
      'Statistics',
      'Files',
      'Map',
      'Getting started',
    ])
      await expect(
        bar.getByRole('link', { name, exact: true }),
        name,
      ).toBeVisible();
    await expect(bar.getByRole('link', { name: 'AI usage' })).toHaveCount(0);

    for (const anchor of [
      'nav.create-task',
      'nav.ask-ai',
      'nav.archive',
      'nav.getting-started',
    ])
      await expect(page.locator(`[data-tour="${anchor}"]`), anchor).toHaveCount(
        1,
      );
    const header = page.locator('header').first();
    await expect(header.locator('[data-tour="nav.create-task"]')).toBeVisible();
    await expect(header.locator('[data-tour="nav.ask-ai"]')).toBeVisible();
    for (const name of ['New task', 'Ask AI'])
      await expect(
        bar.getByRole('button', { name, exact: true }),
        name,
      ).toHaveCount(0);
    await expect(
      page.locator('button[aria-label="Search"]'),
      'one search entry point (the sidebar field)',
    ).toHaveCount(1);

    await bar.getByRole('link', { name: 'Archive', exact: true }).click();
    await expect(page).toHaveURL(/\/demo\/archive$/);
    await expect(
      bar.getByRole('link', { name: 'Archive', exact: true }),
    ).toHaveClass(/router-link-active/);

    await header.getByRole('button', { name: 'New task', exact: true }).click();
    await expect(page.getByPlaceholder('Task name')).toBeVisible();
    await page.keyboard.press('Escape');

    await bar.getByRole('button', { name: 'Search', exact: true }).click();
    await expect(page.getByRole('dialog', { name: 'Search' })).toBeVisible();
    await page.keyboard.press('Escape');

    await header.getByRole('button', { name: 'Ask AI', exact: true }).click();
    await expect(page.locator('[aria-label="AI assistant"]')).toBeVisible();
  });

  test('pinned pages lead the Pages section instead of a group of their own', async ({
    page,
  }) => {
    await webApp(page, {
      pages: [
        pageRow(1, 'Roadmap', 'roadmap', true),
        pageRow(2, 'Scratch', 'scratch'),
      ],
    });
    await page.goto('/demo/board');
    const pages = page.getByTestId('pages-sidebar-section');
    await expect(pages.locator('[data-page-id]')).toHaveCount(2);
    await expect(pages.locator('[data-page-id] a')).toHaveText([
      'Roadmap',
      'Scratch',
    ]);
    await expect(page.getByTestId('sidebar-pinned')).toHaveCount(0);
    await expect(
      sidebar(page).locator('[data-sidebar="group-label"]').first(),
    ).toHaveText('Main');
    await pages.getByRole('link', { name: 'Roadmap' }).click();
    await expect(page).toHaveURL(/\/demo\/pages\/roadmap$/);
  });

  test('pinned pages stay hidden when the pages module is off', async ({
    page,
  }) => {
    await webApp(page, {
      pagesOn: false,
      pages: [pageRow(1, 'Roadmap', 'roadmap', true)],
    });
    await page.goto('/demo/board');
    await expect(
      sidebar(page).getByRole('link', { name: 'Board', exact: true }),
    ).toBeVisible();
    await expect(page.getByTestId('sidebar-pinned')).toHaveCount(0);
    await expect(page.getByTestId('pages-sidebar-section')).toHaveCount(0);
  });

  test('the collapsed icon sidebar keeps a labelled search with a tooltip', async ({
    page,
  }) => {
    await webApp(page);
    await page.goto('/demo/board');
    await expect(
      sidebar(page).getByRole('link', { name: 'Board', exact: true }),
    ).toBeVisible();
    await page.keyboard.press('ControlOrMeta+KeyB');
    await expect(
      page.locator('[data-state="collapsed"][data-collapsible="icon"]'),
    ).toHaveCount(1);
    await expect(
      sidebar(page).getByRole('button', { name: 'Search', exact: true }),
    ).toBeVisible();
    for (const name of ['New task', 'Ask AI'])
      await expect(
        sidebar(page).getByRole('button', { name, exact: true }),
        name,
      ).toHaveCount(0);
    await sidebar(page)
      .getByRole('button', { name: 'Search', exact: true })
      .hover();
    await expect(page.getByRole('tooltip', { name: 'Search' })).toBeVisible();
  });

  // D-012: the settings nav column needs >= 1280px; narrower windows use the
  // list -> section -> back flow. The sidebar/header switch stays at 768px.
  test('1280px gets the settings nav column, 1279px the back link; search moves to the header at 768px', async ({
    page,
  }) => {
    await webApp(page);
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto('/settings?tab=privacy');
    await expect(heading(page)).toHaveText('Privacy');
    await expect(page.getByTestId('settings-layout')).toBeVisible();
    await expect(page.getByTestId('settings-back')).toHaveCount(0);

    for (const width of [1279, 769]) {
      await page.setViewportSize({ width, height: 800 });
      await expect(page.getByTestId('settings-back'), `${width}`).toBeVisible();
      await expect(page.getByTestId('settings-layout')).toHaveCount(0);
      await expect(settingsNav(page)).toHaveCount(0);
      await expect(page.getByTestId('sidebar-quick-actions')).toBeVisible();
      await expect(
        page.getByRole('button', { name: 'New task', exact: true }),
      ).toBeVisible();
    }

    await page.setViewportSize({ width: 768, height: 800 });
    await expect(page.getByTestId('settings-back')).toBeVisible();
    await expect(page.getByTestId('sidebar-quick-actions')).toHaveCount(0);
    await expect(
      page.getByRole('button', { name: 'New task', exact: true }),
    ).toBeVisible();

    await page.setViewportSize({ width: 1024, height: 800 });
    await page.getByTestId('settings-back').click();
    await expect(page).toHaveURL(/\/settings\/sections$/);
    await expect(settingsNav(page)).toBeVisible();
    await page.getByTestId('settings-nav-modules').click();
    await expect(page).toHaveURL(/\/settings\/modules$/);
    await expect(heading(page)).toHaveText('Modules');

    await page.goto('/settings/sections');
    await expect(page).toHaveURL(/\/settings\/sections$/);
    await page.setViewportSize({ width: 1280, height: 800 });
    await expect(page).toHaveURL(/\/settings\/workspaces$/);
    await expect(page.getByTestId('settings-layout')).toBeVisible();
  });

  test('crossing the phone breakpoint keeps the open settings page and its unsaved input', async ({
    page,
  }) => {
    await webApp(page);
    await page.goto('/settings?tab=profile');
    await expect(heading(page)).toHaveText('Profile');
    await page.getByLabel('Name').fill('Unsaved name');

    await page.setViewportSize({ width: 700, height: 800 });
    await expect(page.getByTestId('settings-back')).toBeVisible();
    await expect(page.getByLabel('Name')).toHaveValue('Unsaved name');

    await page.setViewportSize(DESKTOP);
    await expect(page.getByTestId('settings-layout')).toBeVisible();
    await expect(page.getByLabel('Name')).toHaveValue('Unsaved name');
  });

  // Below 1280px (D-012) a settings page gets the full content width and a
  // back link; its heading is visible and wraps to at most two lines.
  for (const width of [820, 1024]) {
    test(`Workspace settings heading stays readable at ${width}px`, async ({
      page,
    }) => {
      await webApp(page);
      await page.setViewportSize({ width, height: 900 });
      await page.goto('/settings/workspaces');
      await expect(page.getByTestId('settings-back')).toBeVisible();
      await expect(page.getByTestId('settings-layout')).toHaveCount(0);
      await expect(heading(page)).toBeVisible();
      const lines = await heading(page).evaluate(
        (h) =>
          h.getBoundingClientRect().height /
          parseFloat(getComputedStyle(h).lineHeight),
      );
      expect(Math.round(lines)).toBeLessThanOrEqual(2);
    });
  }
});

test.describe('phone', () => {
  test.use({ viewport: PHONE });

  test('Settings opens the section list; a section opens alone and Back returns to the list', async ({
    page,
  }) => {
    await webApp(page);
    await page.goto('/demo/board');
    await page.waitForLoadState('networkidle');
    await page.locator('[data-sidebar="trigger"]').first().click();
    const sheet = page.locator('[data-mobile="true"][data-sidebar="sidebar"]');
    await expect(sheet).toBeVisible();
    await openUserMenu(page, sheet);
    await page.getByRole('menuitem', { name: 'Settings' }).click();

    await expect(page).toHaveURL(/\/settings\/sections$/);
    await expect(heading(page)).toHaveText('Settings');
    await expect(settingsNav(page)).toBeVisible();
    await expect(page.getByTestId('settings-back')).toHaveCount(0);
    await expect(page.getByTestId('settings-layout')).toHaveCount(0);
    await expect(groupItems(page, 'account')).toHaveText([
      'Profile',
      'Notifications',
      'Smart Devices',
      'Privacy',
    ]);

    const overflow = await page.evaluate(
      () =>
        document.documentElement.scrollWidth >
        document.documentElement.clientWidth,
    );
    expect(overflow).toBe(false);

    await page.getByTestId('settings-nav-privacy').click();
    await expect(page).toHaveURL(/\/settings\?tab=privacy$/);
    await expect(heading(page)).toHaveText('Privacy');
    await expect(settingsNav(page)).toHaveCount(0);

    await page.getByRole('link', { name: 'Back to Settings' }).click();
    await expect(page).toHaveURL(/\/settings\/sections$/);
    await expect(settingsNav(page)).toBeVisible();

    await page.getByTestId('settings-nav-modules').click();
    await expect(page).toHaveURL(/\/settings\/modules$/);
    await expect(heading(page)).toHaveText('Modules');
    await expect(page.getByTestId('settings-back')).toBeVisible();
  });

  test('the header keeps Search, New task and Ask AI; the sheet opens without them and closes', async ({
    page,
  }) => {
    await webApp(page);
    await page.goto('/demo/board');
    const header = page.locator('header').first();
    await expect(
      header.getByRole('button', { name: 'Search', exact: true }),
    ).toBeVisible();
    await expect(
      header.getByRole('button', { name: 'New task', exact: true }),
    ).toBeVisible();
    await expect(
      header.getByRole('button', { name: 'Ask AI', exact: true }),
    ).toBeVisible();

    await page.waitForLoadState('networkidle');
    await page.locator('[data-sidebar="trigger"]').first().click();
    const sheet = page.locator('[data-mobile="true"][data-sidebar="sidebar"]');
    await expect(sheet).toBeVisible();
    await expect(sheet.getByTestId('sidebar-quick-actions')).toHaveCount(0);
    await expect(
      sheet.getByRole('button', { name: 'New task', exact: true }),
    ).toHaveCount(0);
    await expect(
      sheet.getByRole('button', { name: 'Search', exact: true }),
    ).toHaveCount(0);
    for (const name of ['List', 'Board', 'Archive', 'Statistics', 'Files'])
      await expect(
        sheet.getByRole('link', { name, exact: true }),
        name,
      ).toBeVisible();
    for (const anchor of ['nav.create-task', 'nav.ask-ai', 'nav.archive'])
      await expect(page.locator(`[data-tour="${anchor}"]`), anchor).toHaveCount(
        1,
      );

    await page.keyboard.press('Escape');
    await expect(sheet).toHaveCount(0);
  });
});

test.describe('desktop app', () => {
  test.use({ viewport: DESKTOP });

  test('desktop-only items show in the settings nav and the sidebar', async ({
    page,
  }) => {
    await quietTours(page);
    await desktopPage(page, { onboarding: {} });
    await page.goto('/demo/board');
    await openUserMenu(page);
    await expect(page.getByRole('menuitem')).toHaveText([
      'Profile',
      'Settings',
      'Dark mode',
      'Log out',
    ]);
    await page.getByRole('menuitem', { name: 'Settings' }).click();
    await expect(page).toHaveURL(/\/settings\/workspaces$/);

    await expect(groupItems(page, 'agents')).toHaveText([
      'Agent work',
      'Agent alerts',
      'Plugins',
    ]);
    await expect(groupItems(page, 'app')).toHaveText([
      'Theme',
      'Shortcuts',
      'Experimental',
      'About',
    ]);
    await expect(
      sidebar(page).getByRole('link', { name: 'AI usage', exact: true }),
    ).toBeVisible();
    await expect(page.locator('[data-tour="nav.ask-ai"]')).toHaveCount(0);

    await page.getByTestId('settings-nav-desktop').click();
    await expect(heading(page)).toHaveText('Keyboard shortcuts');
    await expectActive(page, 'desktop');

    await page.getByTestId('settings-nav-plugins').click();
    await expect(page).toHaveURL(/\/settings\/plugins$/);
    await expect(heading(page)).toHaveText('Plugins');
    await expectActive(page, 'plugins');

    await page.goto('/settings/plugins/help');
    await expect(settingsNav(page)).toBeVisible();
    await expectActive(page, 'plugins');
  });

  test('local-only mode has no Log out and no account-only settings', async ({
    page,
  }) => {
    await quietTours(page);
    await desktopPage(
      page,
      { account: false },
      {
        profile: {
          id: -1,
          uuid: '00000000-0000-4000-8000-000000000001',
          name: 'Ada',
          created_at: '2026-10-11T10:00:00Z',
        },
      },
    );
    await page.goto('/');
    await expect(page).toHaveURL(/\/local-my-workspace\//);
    await openUserMenu(page);
    await expect(page.getByRole('menuitem', { name: 'Log out' })).toHaveCount(
      0,
    );
    await expect(page.getByRole('menuitem')).toContainText([
      'Profile',
      'Settings',
    ]);
    await page.getByRole('menuitem', { name: 'Settings' }).click();
    await expect(page).toHaveURL(/\/settings\/workspaces$/);

    await expect(groupItems(page, 'account')).toHaveText([
      'Profile',
      'Privacy',
    ]);
    await expect(groupItems(page, 'agents')).toHaveText(['Plugins']);
    await expect(groupItems(page, 'app')).toContainText([
      'Theme',
      'Local data',
    ]);
    for (const key of [
      'notification',
      'device',
      'agent-work',
      'agent-notifications',
    ])
      await expect(page.getByTestId(`settings-nav-${key}`), key).toHaveCount(0);

    await page.getByTestId('settings-nav-data').click();
    await expect(heading(page)).toHaveText('Local data');
    await expectActive(page, 'data');
  });
});
