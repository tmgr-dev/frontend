import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

const ws = (over = {}) => ({
  id: 1,
  name: 'Demo',
  code: 'demo',
  user_id: 1,
  is_default: true,
  type: 'test',
  ...over,
});

const quietTours = (page) =>
  page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
  });

// A mocked workspace API on top of mockApp: GET list, PUT /workspaces/1.
async function setup(page, { workspaces = [ws()], ignoreColor = false } = {}) {
  await quietTours(page);
  await mockApp(page);
  const state = {
    workspaces,
    puts: [],
    failWith: null,
    gets: 0,
  };
  await page.route('**/api/workspaces', (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    state.gets++;
    return route.fulfill({ json: { data: state.workspaces } });
  });
  await page.route('**/api/workspaces/1', (route) => {
    if (route.request().method() !== 'PUT') return route.fallback();
    const body = route.request().postDataJSON();
    state.puts.push(body);
    if (state.failWith)
      return route.fulfill({
        status: 422,
        json: { message: state.failWith },
      });
    const current = state.workspaces.find((w) => w.id === 1);
    const next = { ...current };
    if (body.name !== undefined) next.name = body.name.trim();
    if (body.color !== undefined && !ignoreColor)
      next.color = body.color === '' ? null : body.color;
    state.workspaces = state.workspaces.map((w) => (w.id === 1 ? next : w));
    return route.fulfill({ json: { data: next } });
  });
  return state;
}

const headerIcon = (page) =>
  page.locator('[data-tour="nav.workspace-switcher"] .aspect-square').first();
const openSwitcher = async (page) => {
  await page.getByTitle('Switch workspace').first().click();
  return page.getByRole('menuitem', { name: /Demo|Renamed/ }).first();
};
const switcherIcon = (item) => item.locator('div.size-6').first();
const section = (page) => page.getByTestId('workspace-general');
const nameInput = (page) => section(page).getByLabel('Name');
const save = (page) => page.getByTestId('workspace-save');

test('owner renames: sidebar, switcher, breadcrumbs and title follow without a reload', async ({
  page,
}) => {
  const state = await setup(page);
  await page.goto('/settings/workspaces');
  await expect(nameInput(page)).toHaveValue('Demo');
  await expect(save(page)).toBeDisabled();

  await page.evaluate(() => {
    window.__noReload = true;
  });

  await nameInput(page).fill('  Renamed HQ  ');
  await expect(save(page)).toBeEnabled();
  await save(page).click();

  await expect(
    page.getByText('Workspace saved', { exact: true }),
  ).toBeVisible();
  expect(state.puts).toEqual([{ name: 'Renamed HQ' }]);
  await expect(
    page
      .locator('[data-tour="nav.workspace-switcher"]')
      .getByText('Renamed HQ'),
  ).toBeVisible();
  await expect(nameInput(page)).toHaveValue('Renamed HQ');
  await expect(save(page)).toBeDisabled();

  const item = await openSwitcher(page);
  await expect(item).toContainText('Renamed HQ');
  await page.keyboard.press('Escape');

  await page
    .locator('[data-sidebar="sidebar"]')
    .first()
    .getByRole('link', { name: 'Dashboard' })
    .click();
  await expect(page).toHaveURL(/\/demo\/dashboard/);
  await expect(
    page.getByRole('navigation', { name: /breadcrumb/i }),
  ).toContainText('Renamed HQ Dashboard');
  await expect(page).toHaveTitle(/Renamed HQ Dashboard/);
  expect(await page.evaluate(() => window.__noReload)).toBe(true);
});

test('a blank name is blocked: hint shown, Save disabled, nothing sent', async ({
  page,
}) => {
  const state = await setup(page);
  await page.goto('/settings/workspaces');
  await nameInput(page).fill('   ');
  await expect(page.getByTestId('workspace-name-hint')).toHaveText(
    "Name can't be empty",
  );
  await expect(save(page)).toBeDisabled();
  await nameInput(page).press('Enter');
  expect(state.puts).toEqual([]);
  await expect(
    page.locator('[data-tour="nav.workspace-switcher"]').getByText('Demo'),
  ).toBeVisible();
});

test('a 422 shows the server message and leaves the name unchanged', async ({
  page,
}) => {
  const state = await setup(page);
  state.failWith = 'The name has already been taken.';
  await page.goto('/settings/workspaces');
  await nameInput(page).fill('Taken');
  await save(page).click();

  await expect(
    page.getByText('The name has already been taken.', { exact: true }),
  ).toBeVisible();
  expect(state.puts).toEqual([{ name: 'Taken' }]);
  await expect(
    page.locator('[data-tour="nav.workspace-switcher"]').getByText('Demo'),
  ).toBeVisible();
  await expect(nameInput(page)).toHaveValue('Taken');
  await expect(save(page)).toBeEnabled();
});

test('picking a colour recolours the icons; Default restores today’s look', async ({
  page,
}) => {
  const state = await setup(page);
  await page.goto('/settings/workspaces');

  const group = page.getByRole('radiogroup', { name: 'Colour' });
  await expect(group.getByRole('radio', { name: 'Default' })).toHaveAttribute(
    'aria-checked',
    'true',
  );
  await expect(headerIcon(page)).not.toHaveAttribute('style', /./);

  await group.getByRole('radio', { name: 'Blue' }).click();
  await expect(group.getByRole('radio', { name: 'Blue' })).toHaveAttribute(
    'aria-checked',
    'true',
  );
  await save(page).click();
  await expect(
    page.getByText('Workspace saved', { exact: true }),
  ).toBeVisible();
  expect(state.puts).toEqual([{ color: '#3B82F6' }]);

  await expect(headerIcon(page)).toHaveCSS(
    'background-color',
    'rgb(59, 130, 246)',
  );
  await expect(headerIcon(page)).toHaveCSS('color', 'rgb(0, 0, 0)');
  let item = await openSwitcher(page);
  await expect(switcherIcon(item)).toHaveCSS(
    'background-color',
    'rgb(59, 130, 246)',
  );
  await page.keyboard.press('Escape');

  await group.getByRole('radio', { name: 'Default' }).click();
  await save(page).click();
  await expect.poll(() => state.puts.length).toBe(2);
  expect(state.puts[1]).toEqual({ color: '' });
  await expect(headerIcon(page)).not.toHaveAttribute('style', /./);
  item = await openSwitcher(page);
  await expect(switcherIcon(item)).not.toHaveAttribute('style', /./);
});

test('a dark colour gets a white icon foreground', async ({ page }) => {
  await setup(page, { workspaces: [ws({ color: '#1E3A8A' })] });
  await page.goto('/settings/workspaces');
  await expect(headerIcon(page)).toHaveCSS(
    'background-color',
    'rgb(30, 58, 138)',
  );
  await expect(headerIcon(page)).toHaveCSS('color', 'rgb(255, 255, 255)');
});

test('color missing or null renders the default icon', async ({ page }) => {
  await setup(page, {
    workspaces: [
      ws(),
      ws({ id: 2, name: 'Other', code: 'other', color: null }),
    ],
  });
  await page.goto('/settings/workspaces');
  await expect(page.getByRole('radio', { name: 'Default' })).toHaveAttribute(
    'aria-checked',
    'true',
  );
  await expect(headerIcon(page)).not.toHaveAttribute('style', /./);
  await page.getByTitle('Switch workspace').first().click();
  const other = page.getByRole('menuitem', { name: /Other/ });
  await expect(switcherIcon(other)).not.toHaveAttribute('style', /./);
});

test('a server that ignores color keeps the default look', async ({ page }) => {
  const state = await setup(page, { ignoreColor: true });
  await page.goto('/settings/workspaces');
  await page.getByRole('radio', { name: 'Red' }).click();
  await save(page).click();
  await expect(
    page.getByText('Workspace saved', { exact: true }),
  ).toBeVisible();
  expect(state.puts).toEqual([{ color: '#EF4444' }]);
  await expect(headerIcon(page)).not.toHaveAttribute('style', /./);
  await expect(page.getByRole('radio', { name: 'Default' })).toHaveAttribute(
    'aria-checked',
    'true',
  );
});

test('a non-owner sees the name read-only: no inputs, no picker, no Save', async ({
  page,
}) => {
  await setup(page, { workspaces: [ws({ user_id: 99, color: '#22C55E' })] });
  await page.goto('/settings/workspaces');
  await expect(page.getByTestId('workspace-name-readonly')).toHaveText('Demo');
  await expect(section(page).getByRole('textbox')).toHaveCount(0);
  await expect(page.getByRole('radiogroup')).toHaveCount(0);
  await expect(save(page)).toHaveCount(0);
  await expect(headerIcon(page)).toHaveCSS(
    'background-color',
    'rgb(34, 197, 94)',
  );
});

test('a local workspace has no rename or colour fields', async ({ page }) => {
  await quietTours(page);
  await desktopPage(page);
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);

  await page.goto('/settings/workspaces');
  await expect(page.getByTestId('local-workspace-badge').first()).toBeVisible();
  await expect(
    page.getByText('Used by Telegram, MCP, smart devices'),
  ).toBeVisible();
  await expect(section(page)).toHaveCount(0);
  await expect(page.getByRole('radiogroup')).toHaveCount(0);
});

test('swatches work from the keyboard and name + colour go in one PUT', async ({
  page,
}) => {
  const state = await setup(page);
  await page.goto('/settings/workspaces');
  const group = page.getByRole('radiogroup', { name: 'Colour' });
  await group.getByRole('radio', { name: 'Default' }).focus();
  await page.keyboard.press('Tab');
  await expect(group.getByRole('radio', { name: 'Red' })).toBeFocused();
  await page.keyboard.press('Space');
  await expect(group.getByRole('radio', { name: 'Red' })).toHaveAttribute(
    'aria-checked',
    'true',
  );
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');
  await expect(group.getByRole('radio', { name: 'Orange' })).toHaveAttribute(
    'aria-checked',
    'true',
  );
  await nameInput(page).fill('Both');
  await save(page).click();
  await expect(
    page.getByText('Workspace saved', { exact: true }),
  ).toBeVisible();
  expect(state.puts).toEqual([{ name: 'Both', color: '#F97316' }]);
});

test('spaces around the current name are not a change', async ({ page }) => {
  const state = await setup(page);
  await page.goto('/settings/workspaces');
  await nameInput(page).fill('  Demo  ');
  await expect(save(page)).toBeDisabled();
  expect(state.puts).toEqual([]);
});

test('a renamed workspace shows under its new name in the command palette', async ({
  page,
}) => {
  const state = await setup(page, {
    workspaces: [ws(), ws({ id: 2, name: 'Other', code: 'other' })],
  });
  await page.goto('/settings/workspaces');
  await nameInput(page).fill('Renamed HQ');
  await save(page).click();
  await expect(
    page.getByText('Workspace saved', { exact: true }),
  ).toBeVisible();
  expect(state.puts).toEqual([{ name: 'Renamed HQ' }]);

  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /Other/ }).click();
  await expect(
    page.locator('[data-tour="nav.workspace-switcher"]').getByText('Other'),
  ).toBeVisible();
  await page.keyboard.press('ControlOrMeta+k');
  await expect(page.getByTestId('command-palette-input')).toBeFocused();
  await page.getByTestId('command-palette-input').fill('renamed');
  await expect(
    page.locator(
      '[data-testid="command-palette-item"][data-group="workspaces"]',
    ),
  ).toContainText('Renamed HQ');
});
