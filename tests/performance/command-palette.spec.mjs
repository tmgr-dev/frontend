import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const setup = async (page, { failSearch = false, mock = {} } = {}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page, mock);
  await page.route('**/api/workspaces', (route) =>
    route.fulfill({
      json: {
        data: [
          { id: 1, name: 'Demo', code: 'demo', user_id: 1, is_default: true },
          { id: 2, name: 'Side project', code: 'side', user_id: 1 },
        ],
      },
    }),
  );
  await page.route('**/api/tasks/current**', async (route) => {
    const search = new URL(route.request().url()).searchParams.get('search');
    if (!search) return route.fallback();
    if (failSearch) return route.fulfill({ status: 500, json: {} });
    const data = search.includes('zzz')
      ? []
      : [
          { id: 1, title: 'Original task', category: null },
          { id: 2, title: 'Fix login redirect', category: null },
        ];
    await route.fulfill({
      json: {
        data,
        meta: { current_page: 1, per_page: 10, total: data.length },
      },
    });
  });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().waitFor({ timeout: 60000 });
  return errors;
};

const palette = (page) => page.getByTestId('command-palette');
const input = (page) => page.getByTestId('command-palette-input');
const items = (page, group) =>
  page.locator(`[data-testid="command-palette-item"][data-group="${group}"]`);

const openPalette = async (page) => {
  await page.keyboard.press('ControlOrMeta+k');
  await expect(palette(page)).toBeVisible();
  await expect(input(page)).toBeFocused();
};

const modalTaskId = (page) =>
  page.evaluate(
    () =>
      document.querySelector('#app').__vue_app__.config.globalProperties.$store
        .state.currentTaskIdForModal,
  );

test('hotkey, type, Enter opens the task', async ({ page }) => {
  const errors = await setup(page);
  await openPalette(page);
  await input(page).fill('fix');
  await expect(items(page, 'tasks')).toHaveCount(2);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/demo\/tasks\/2$/);
  await expect(palette(page)).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('Navigation item navigates; the wrong modifier does nothing', async ({
  page,
}) => {
  await setup(page);
  await page.keyboard.press(
    process.platform === 'darwin' ? 'Control+k' : 'Meta+k',
  );
  await page.waitForTimeout(200);
  await expect(palette(page)).toHaveCount(0);
  await openPalette(page);
  await input(page).fill('board');
  await expect(items(page, 'navigation').first()).toHaveText(/Board/);
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/demo\/board$/);
});

test('empty state: Nothing found', async ({ page }) => {
  await setup(page);
  await openPalette(page);
  await input(page).fill('zzz');
  await expect(page.getByTestId('command-palette-empty')).toHaveText(
    'Nothing found',
  );
});

test('search failure keeps Navigation', async ({ page }) => {
  await setup(page, { failSearch: true });
  await openPalette(page);
  await input(page).fill('ar');
  await expect(page.getByTestId('command-palette-status')).toHaveText(
    'Search failed',
  );
  await expect(items(page, 'navigation').first()).toHaveText(/Archive/);
});

test('Esc returns focus to the input; the hotkey toggles it closed', async ({
  page,
}) => {
  await setup(page);
  const field = page.locator('input:visible').first();
  await field.focus();
  await openPalette(page);
  await page.keyboard.press('Escape');
  await expect(palette(page)).toHaveCount(0);
  await expect(field).toBeFocused();
  await openPalette(page);
  await page.keyboard.press('ControlOrMeta+k');
  await expect(palette(page)).toHaveCount(0);
  await expect(field).toBeFocused();
});

test('Esc over the task modal closes only the palette', async ({ page }) => {
  await setup(page);
  await page.locator('[data-task-id="1"]').first().click();
  await page.getByPlaceholder('Task name').waitFor();
  await openPalette(page);
  await page.keyboard.press('Escape');
  expect(await modalTaskId(page)).not.toBeNull();
  await expect(palette(page)).toHaveCount(0);
  await page.waitForTimeout(300);
  expect(await modalTaskId(page)).not.toBeNull();
});

test('sidebar trigger opens the palette and shows the hint', async ({
  page,
}) => {
  await setup(page);
  const trigger = page.getByTestId('command-palette-trigger');
  await expect(trigger).toContainText(/⌘K|Ctrl K/);
  await trigger.click();
  await expect(input(page)).toBeFocused();
  await input(page).fill('side');
  await items(page, 'workspaces').first().click();
  await expect(page).toHaveURL(/\/side\/list$/);
});

test('Cmd/Ctrl+Enter opens the task in a new tab', async ({
  page,
  context,
}) => {
  await setup(page);
  await openPalette(page);
  await input(page).fill('ori');
  await expect(items(page, 'tasks')).toHaveCount(2);
  const popup = context.waitForEvent('page');
  await page.keyboard.press('ControlOrMeta+Enter');
  expect((await popup).url()).toMatch(/\/demo\/tasks\/1$/);
  await expect(page).toHaveURL(/\/demo\/list$/);
});

test('Editor.js keeps the hotkey for its link tool', async ({ page }) => {
  await page.addInitScript(() =>
    localStorage.setItem('preferred_editor', 'block'),
  );
  await setup(page, { mock: { blockTask: true } });
  await page.locator('[data-task-id="1"]').first().click();
  await page.getByRole('button', { name: 'Edit description' }).click();
  const block = page.locator('.codex-editor .ce-paragraph').first();
  await expect(block).toHaveText('Initial block');
  await block.click();
  await block.evaluate((el) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    getSelection().removeAllRanges();
    getSelection().addRange(range);
  });
  await expect(
    page.locator('.ce-inline-toolbar .ce-popover--opened'),
  ).toBeAttached();
  await page.keyboard.press('ControlOrMeta+k');
  await expect(page.locator('.ce-inline-tool-input--showed')).toBeVisible();
  await expect(palette(page)).toHaveCount(0);

  await page.getByPlaceholder('Task name').focus();
  await openPalette(page);
});
