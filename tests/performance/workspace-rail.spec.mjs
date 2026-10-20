import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

const ws = (id, name, extra = {}) => ({
  id,
  name,
  code: name.toLowerCase().replace(/[^a-z0-9]+/g, '-'),
  user_id: 1,
  is_default: id === 1,
  ...extra,
});

const THREE = [ws(1, 'Demo'), ws(2, 'Acme Corp'), ws(3, 'Side Project')];
const DESKTOP = { width: 1440, height: 900 };
const PHONE = { width: 390, height: 844 };

const rail = (page) => page.getByTestId('workspace-rail');
const chips = (page) => page.getByTestId('workspace-rail-chip');
const chip = (page, name) =>
  page.locator(`[data-testid="workspace-rail-chip"][aria-label="${name}"]`);
const header = (page) =>
  page.locator('[data-sidebar="header"] [data-sidebar="menu-button"]').first();
const tourCount = (page, anchor) =>
  page.locator(`[data-tour="${anchor}"]`).count();

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
});

// A click right after first paint can be lost while the sidebar is still loading.
const open = async (page, path) => {
  await page.goto(path);
  await page.waitForLoadState('networkidle');
};

// Vue warns are counted only when they come from the rail or the user menu: other
// components warn on develop too. The fake desktop shell has no event-plugin
// internals, so Tauri's unlisten throws there.
function trackErrors(page) {
  const errors = [];
  const shellNoise = /reading 'unregisterListener'/;
  page.on('pageerror', (error) => {
    if (!shellNoise.test(error.message)) errors.push(error.message);
  });
  page.on('console', (message) => {
    const text = message.text();
    if (shellNoise.test(text)) return;
    if (message.type() === 'error') errors.push(text);
    else if (
      /\[Vue warn\]/.test(text) &&
      /<(WorkspaceRail|SidebarUserMenu)\b/.test(text)
    )
      errors.push(text);
  });
  return errors;
}

async function webApp(page, workspaces = THREE, options = {}) {
  const errors = trackErrors(page);
  const exits = [];
  await mockApp(page, { taskCount: 2, ...options });
  await page.route(/\/api\/workspaces(\?.*)?$/, (route) =>
    route.fulfill({ json: { data: workspaces } }),
  );
  await page.route(/\/api\/workspaces\/\d+\/exit$/, (route) => {
    exits.push(new URL(route.request().url()).pathname);
    return route.fulfill({ json: { data: null } });
  });
  return { errors, exits };
}

test.describe('desktop web, expanded', () => {
  test.use({ viewport: DESKTOP });

  test('the rail sits at the left edge with one chip per workspace, the active one marked', async ({
    page,
  }) => {
    const { errors } = await webApp(page);
    await open(page, '/demo/list');
    await expect(rail(page)).toBeVisible();
    await expect(rail(page)).toHaveAttribute('aria-label', 'Workspaces');

    const box = await rail(page).boundingBox();
    expect(box.x).toBe(0);
    expect(Math.round(box.width)).toBe(56);
    const panel = await header(page).boundingBox();
    expect(panel.x).toBeGreaterThanOrEqual(box.x + box.width);

    await expect(chips(page)).toHaveCount(3);
    expect(
      await chips(page).evaluateAll((els) =>
        els.map((el) => el.getAttribute('aria-label')),
      ),
    ).toEqual(['Demo', 'Acme Corp', 'Side Project']);
    await expect(chip(page, 'Demo')).toHaveAttribute('aria-current', 'true');
    await expect(chip(page, 'Demo')).toHaveAttribute('data-active', 'true');
    await expect(
      page.locator('[aria-current="true"]').and(chips(page)),
    ).toHaveCount(1);
    for (const name of ['Acme Corp', 'Side Project'])
      await expect(chip(page, name)).not.toHaveAttribute('aria-current');

    const bg = (name) =>
      chip(page, name).evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(await bg('Demo')).not.toBe(await bg('Acme Corp'));
    await expect(
      page.locator('li:has([aria-current="true"]) > span[aria-hidden="true"]'),
    ).toBeVisible();

    await chip(page, 'Acme Corp').hover();
    await expect(
      page.getByRole('tooltip', { name: 'Acme Corp' }),
    ).toBeVisible();

    await expect(page.getByTitle('Switch workspace')).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test('clicking a chip switches the workspace; the active chip does nothing', async ({
    page,
  }) => {
    const { errors } = await webApp(page);
    await open(page, '/demo/list');
    await expect(chip(page, 'Demo')).toHaveAttribute('aria-current', 'true');

    await chip(page, 'Demo').click();
    await expect(page).toHaveURL(/\/demo\/list$/);

    await chip(page, 'Acme Corp').click();
    await expect(page).toHaveURL(/\/acme-corp\/list$/);
    await expect(header(page)).toContainText('Acme Corp');
    await expect(chip(page, 'Acme Corp')).toHaveAttribute(
      'aria-current',
      'true',
    );
    await expect(chip(page, 'Demo')).not.toHaveAttribute('aria-current');
    expect(errors).toEqual([]);
  });

  test('chips, + and the avatar are keyboard operable with a visible focus ring', async ({
    page,
  }) => {
    const { errors } = await webApp(page);
    await open(page, '/demo/list');
    await expect(chip(page, 'Demo')).toBeVisible();

    await chip(page, 'Demo').focus();
    await page.keyboard.press('Tab');
    await expect(chip(page, 'Acme Corp')).toBeFocused();
    expect(
      await chip(page, 'Acme Corp').evaluate(
        (el) => getComputedStyle(el).boxShadow,
      ),
    ).not.toBe('none');
    await page.keyboard.press('Enter');
    await expect(page).toHaveURL(/\/acme-corp\/list$/);
    await expect(chip(page, 'Acme Corp')).toHaveAttribute(
      'aria-current',
      'true',
    );

    await chip(page, 'Side Project').focus();
    await page.keyboard.press('Space');
    await expect(page).toHaveURL(/\/side-project\/list$/);

    const add = rail(page).getByRole('button', { name: 'Add workspace' });
    const user = rail(page).getByRole('button', { name: 'User menu' });
    await add.focus();
    await page.keyboard.press('Enter');
    await expect(
      page.getByRole('menuitem', { name: 'Add workspace' }),
    ).toBeVisible();
    await page.keyboard.press('Escape');
    await user.focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('menuitem', { name: 'Log out' })).toBeVisible();
    expect(errors).toEqual([]);
  });

  test('the header keeps workspace actions only; Leave shows for a leavable workspace', async ({
    page,
  }) => {
    const { errors, exits } = await webApp(page);
    await open(page, '/demo/list');
    await page.getByTitle('Workspace actions').click();
    const menu = page.getByRole('menu');
    await expect(menu.getByRole('menuitem')).toHaveText(['Invite members']);
    await page.keyboard.press('Escape');

    await chip(page, 'Acme Corp').click();
    await expect(page).toHaveURL(/\/acme-corp\/list$/);
    await page.getByTitle('Workspace actions').click();
    await expect(page.getByRole('menu').getByRole('menuitem')).toHaveText([
      'Invite members',
      'Leave this workspace',
    ]);
    await page.getByTestId('leave-workspace-menu-item').click();
    await expect(
      page.getByText("Are you sure you want to leave 'Acme Corp' workspace?"),
    ).toBeVisible();
    await page.getByRole('button', { name: 'Ok', exact: true }).click();
    await expect.poll(() => exits).toEqual(['/api/workspaces/2/exit']);
    expect(errors).toEqual([]);
  });

  test('Invite members and the workspace name still navigate', async ({
    page,
  }) => {
    const { errors } = await webApp(page);
    await open(page, '/demo/list');
    await page.getByTitle('Workspace actions').click();
    await page.getByRole('menuitem', { name: 'Invite members' }).click();
    await expect(page).toHaveURL(/\/settings\/workspaces\?invite$/);

    // The name opens /app, which lands on the workspace's default page (list here).
    await open(page, '/demo/board');
    await header(page).getByText('Demo', { exact: true }).click();
    await expect(page).toHaveURL(/\/demo\/list$/);
    expect(errors).toEqual([]);
  });

  test('+ offers Add workspace on the web, which opens the create form', async ({
    page,
  }) => {
    const { errors } = await webApp(page);
    await open(page, '/demo/list');
    await page.getByTestId('workspace-rail-add').click();
    await expect(page.getByRole('menu').getByRole('menuitem')).toHaveText([
      'Add workspace',
    ]);
    await expect(page.getByTestId('workspace-rail-add-local')).toHaveCount(0);
    await page.getByTestId('workspace-rail-add-workspace').click();
    await expect(page).toHaveURL(/\/settings\/workspaces\?create$/);
    expect(errors).toEqual([]);
  });

  test('the rail avatar opens the user menu and the panel footer has none', async ({
    page,
  }) => {
    const { errors } = await webApp(page);
    await open(page, '/demo/list');
    await expect(rail(page)).toBeVisible();
    await expect(page.locator('[data-sidebar="footer"]')).toHaveCount(0);
    await page.getByTestId('workspace-rail-user').click();
    await expect(
      page.getByRole('menu').getByText('test@example.test'),
    ).toBeVisible();
    await expect(page.getByRole('menuitem')).toHaveText([
      'Profile',
      'Settings',
      'Dark mode',
      'Log out',
    ]);
    await page.getByRole('menuitem').getByText('Dark mode').click();
    await expect(page.locator('html')).toHaveClass(/\bdark\b/);
    expect(errors).toEqual([]);
  });

  test('many workspaces scroll inside the rail; + and the avatar stay visible', async ({
    page,
  }) => {
    const many = Array.from({ length: 24 }, (_, i) =>
      i === 0 ? ws(1, 'Demo') : ws(i + 1, `Team ${i + 1}`),
    );
    const { errors } = await webApp(page, many);
    await open(page, '/demo/list');
    await expect(chips(page)).toHaveCount(24);
    const list = rail(page).locator('ul');
    const { scroll, client } = await list.evaluate((el) => ({
      scroll: el.scrollHeight,
      client: el.clientHeight,
    }));
    expect(scroll).toBeGreaterThan(client);
    expect(
      await page.evaluate(
        () =>
          document.documentElement.scrollHeight <=
          document.documentElement.clientHeight,
      ),
    ).toBe(true);
    await expect(page.getByTestId('workspace-rail-add')).toBeInViewport({
      ratio: 1,
    });
    await expect(page.getByTestId('workspace-rail-user')).toBeInViewport({
      ratio: 1,
    });
    await expect(chip(page, 'Team 24')).not.toBeInViewport();

    await chip(page, 'Team 24').scrollIntoViewIfNeeded();
    await expect(chip(page, 'Team 24')).toBeInViewport();
    await expect(page.getByTestId('workspace-rail-add')).toBeInViewport({
      ratio: 1,
    });
    await chip(page, 'Team 24').click();
    await expect(page).toHaveURL(/\/team-24\/list$/);
    expect(errors).toEqual([]);
  });

  test('collapsed (Cmd+B) has no rail and today’s icon column; expanding brings it back', async ({
    page,
  }) => {
    const { errors } = await webApp(page);
    await open(page, '/demo/list');
    await expect(rail(page)).toBeVisible();
    await page.keyboard.press('ControlOrMeta+KeyB');
    await expect(
      page.locator('[data-state="collapsed"][data-collapsible="icon"]'),
    ).toHaveCount(1);
    await expect(rail(page)).toHaveCount(0);
    expect(await tourCount(page, 'nav.workspace-switcher')).toBe(1);
    expect(await tourCount(page, 'nav.user-menu')).toBe(1);

    await page.getByTitle('Switch workspace').first().click();
    const menu = page.getByRole('menu');
    for (const name of ['Demo', 'Acme Corp', 'Side Project', 'Add workspace'])
      await expect(menu.getByRole('menuitem', { name })).toBeVisible();
    await page.keyboard.press('Escape');

    await page
      .locator('[data-sidebar="footer"] [aria-haspopup="menu"]')
      .click();
    await expect(page.getByRole('menuitem', { name: 'Log out' })).toBeVisible();
    await page.keyboard.press('Escape');

    await page.keyboard.press('ControlOrMeta+KeyB');
    await expect(rail(page)).toBeVisible();
    await expect(chips(page)).toHaveCount(3);
    expect(errors).toEqual([]);
  });

  test('one workspace: one active chip, + and the avatar', async ({ page }) => {
    const { errors } = await webApp(page, [ws(1, 'Demo')]);
    await open(page, '/demo/list');
    await expect(chips(page)).toHaveCount(1);
    await expect(chip(page, 'Demo')).toHaveAttribute('aria-current', 'true');
    await expect(page.getByTestId('workspace-rail-add')).toBeVisible();
    await expect(page.getByTestId('workspace-rail-user')).toBeVisible();
    expect(errors).toEqual([]);
  });

  test('no workspaces yet: the rail has only + and the avatar', async ({
    page,
  }) => {
    const { errors } = await webApp(page, []);
    await open(page, '/settings/workspaces');
    await expect(rail(page)).toBeVisible();
    await expect(chips(page)).toHaveCount(0);
    await expect(page.getByTestId('workspace-rail-add')).toBeVisible();
    await expect(page.getByTestId('workspace-rail-user')).toBeVisible();
    expect(errors).toEqual([]);
  });

  test('each tour anchor exists once while the rail is shown', async ({
    page,
  }) => {
    await webApp(page);
    await open(page, '/demo/list');
    await expect(rail(page)).toBeVisible();
    for (const anchor of [
      'nav.workspace-switcher',
      'nav.user-menu',
      'nav.sidebar',
    ])
      expect(await tourCount(page, anchor), anchor).toBe(1);
    await expect(
      rail(page).locator('[data-tour="nav.workspace-switcher"]'),
    ).toHaveCount(1);
    await expect(rail(page).locator('[data-tour="nav.user-menu"]')).toHaveCount(
      1,
    );
  });
});

test.describe('phone', () => {
  test.use({ viewport: PHONE });

  test('no rail; the sheet keeps the full switcher and it switches', async ({
    page,
  }) => {
    const { errors } = await webApp(page);
    await open(page, '/demo/list');
    await page.waitForLoadState('networkidle');
    await expect(rail(page)).toHaveCount(0);
    await page.locator('[data-sidebar="trigger"]').first().click();
    const sheet = page.locator('[data-mobile="true"][data-sidebar="sidebar"]');
    await expect(sheet).toBeVisible();
    await expect(rail(page)).toHaveCount(0);
    expect(await tourCount(page, 'nav.workspace-switcher')).toBe(1);
    expect(await tourCount(page, 'nav.user-menu')).toBe(1);

    await sheet.getByTitle('Switch workspace').first().click();
    const menu = page.getByRole('menu');
    for (const name of ['Demo', 'Acme Corp', 'Side Project', 'Add workspace'])
      await expect(menu.getByRole('menuitem', { name })).toBeVisible();
    await menu.getByRole('menuitem', { name: 'Acme Corp' }).click();
    await expect(page).toHaveURL(/\/acme-corp\/list$/);
    expect(errors).toEqual([]);
  });
});

test.describe('desktop app', () => {
  test.use({ viewport: DESKTOP });

  test('the rail starts below the titlebar and leaves the header drag region alone', async ({
    page,
  }) => {
    const errors = trackErrors(page);
    await desktopPage(page);
    await open(page, '/demo/board');
    await expect(page.locator('html')).toHaveClass(/tauri-desktop/);
    await expect(rail(page)).toBeVisible();
    const titlebar = await page.evaluate(() =>
      parseFloat(
        getComputedStyle(document.documentElement).getPropertyValue(
          '--titlebar-h',
        ),
      ),
    );
    expect(titlebar).toBe(40);
    const box = await rail(page).boundingBox();
    expect(Math.round(box.y)).toBe(titlebar);
    const drag = page.locator('header[data-tauri-drag-region]');
    const headerBox = await drag.boundingBox();
    expect(headerBox.y).toBe(0);
    expect(Math.round(headerBox.height)).toBe(titlebar);
    expect(
      await page.evaluate((y) => {
        const el = document.elementFromPoint(28, y);
        return !!el?.closest('[data-tauri-drag-region]');
      }, titlebar / 2),
    ).toBe(true);

    // The status bar starts at the panel's right edge: rail + panel, then the icon column.
    const statusLeft = () =>
      page
        .locator('.statusbar-offset')
        .evaluate((el) => Math.round(el.getBoundingClientRect().left));
    await expect.poll(statusLeft).toBe(312);
    await page.keyboard.press('ControlOrMeta+KeyB');
    await expect(rail(page)).toHaveCount(0);
    await expect.poll(statusLeft).toBe(48);
    expect(errors).toEqual([]);
  });

  test('+ creates a local workspace; its chip and header show it is local', async ({
    page,
  }) => {
    const errors = trackErrors(page);
    await page.addInitScript(() =>
      localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1'),
    );
    await desktopPage(page);
    await open(page, '/demo/board');
    await page.getByTestId('workspace-rail-add').click();
    await expect(page.getByRole('menu').getByRole('menuitem')).toHaveText([
      'Add workspace',
      'New local workspace',
    ]);
    await page.getByTestId('workspace-rail-add-local').click();
    await page.getByPlaceholder(/Personal, Client/).fill('Personal');
    await page.getByRole('button', { name: 'Create' }).click();
    await expect(page).toHaveURL(/\/local-personal\//);

    await expect(chips(page)).toHaveCount(2);
    await expect(
      chips(page).and(page.locator('[aria-current="true"]')),
    ).toHaveAccessibleName(/\(Local\)$/);
    await expect(chip(page, 'Demo')).toHaveAccessibleName('Demo');
    await expect(chip(page, 'Personal (Local)')).toHaveAttribute(
      'aria-current',
      'true',
    );
    await expect(
      chip(page, 'Personal (Local)').getByTestId('workspace-rail-local'),
    ).toBeVisible();
    await expect(page.getByTestId('local-workspace-badge')).toBeVisible();
    expect(await tourCount(page, 'local.badge')).toBe(1);

    await page.getByTitle('Workspace actions').click();
    const items = page.getByRole('menu').getByRole('menuitem');
    await expect(items).toContainText([
      'Export to Markdown',
      'Move to cloud…',
      'Invite members',
    ]);
    await expect(
      page.getByRole('menuitem', { name: 'New local workspace' }),
    ).toHaveCount(0);
    await expect(
      page.getByRole('menuitem', { name: 'Leave this workspace' }),
    ).toHaveCount(0);
    await page.keyboard.press('Escape');

    await chip(page, 'Demo').click();
    await expect(page).toHaveURL(/\/demo\//);
    await expect(page.getByTestId('local-workspace-badge')).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test('local-only mode lists local workspaces, no Log out, Create account reachable', async ({
    page,
  }) => {
    const errors = trackErrors(page);
    await page.addInitScript(() =>
      localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1'),
    );
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
    await open(page, '/');
    await expect(page).toHaveURL(/\/local-my-workspace\//);
    await expect(rail(page)).toBeVisible();
    await expect(chips(page)).toHaveCount(1);
    await expect(chip(page, 'My workspace (Local)')).toHaveAttribute(
      'aria-current',
      'true',
    );
    await expect(
      chip(page, 'My workspace (Local)').getByTestId('workspace-rail-local'),
    ).toBeVisible();
    await expect(page.getByTestId('local-workspace-badge')).toBeVisible();
    await expect(page.getByTestId('sidebar-create-account')).toBeVisible();
    for (const anchor of [
      'nav.workspace-switcher',
      'nav.user-menu',
      'nav.create-account',
      'local.badge',
    ])
      expect(await tourCount(page, anchor), anchor).toBe(1);

    await page.getByTestId('workspace-rail-user').click();
    await expect(
      page.getByRole('menuitem', { name: 'Settings' }),
    ).toBeVisible();
    await expect(page.getByRole('menuitem', { name: 'Log out' })).toHaveCount(
      0,
    );
    await page.keyboard.press('Escape');

    await page.getByTestId('sidebar-create-account').click();
    await expect(page).toHaveURL(/\/login/);
    expect(errors).toEqual([]);
  });
});
