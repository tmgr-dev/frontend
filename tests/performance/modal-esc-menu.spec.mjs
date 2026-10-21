import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

// TM-706: in the task modal, Esc with an open dropdown menu or select listbox closes only that
// layer; the next Esc closes the modal. Date.now is frozen, so a first Esc that armed the 1s close
// debounce would block every later close: the modal closing on the second Esc proves it did not.

const OPEN_MENU = '[role="menu"][data-state="open"]';
const OPEN_LISTBOX = '[role="listbox"][data-state="open"]';

const openTaskModal = async (page) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await page.clock.setFixedTime(new Date('2026-10-21T10:00:00Z'));
  await mockApp(page);
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click({ timeout: 60000 });
  await page.getByPlaceholder('Task name').waitFor();
  const panel = page.locator('[data-name="task-side-panel"]');
  await expect(panel).toHaveCount(1);
  return panel;
};

// Read right after the key press: the Esc handler closes the modal synchronously through the store
// (the panel itself lingers in its leave transition), and the layer must not stay "open" during
// its own exit animation.
const afterEsc = (page, role) =>
  page.evaluate((r) => {
    const store =
      document.querySelector('#app').__vue_app__.config.globalProperties.$store;
    return {
      layer: document.querySelector(`[role="${r}"]`)?.dataset.state ?? 'gone',
      modalOpen: store.state.currentTaskIdForModal != null,
    };
  }, role);

const escLayerThenModal = async (page, panel, role, openSelector) => {
  await page.keyboard.press('Escape');
  const { layer, modalOpen } = await afterEsc(page, role);
  expect(['closed', 'gone']).toContain(layer);
  expect(modalOpen, 'modal still open after the first Esc').toBe(true);
  await expect(page.locator(openSelector)).toHaveCount(0);
  await expect(panel).toHaveCount(1);
  await page.keyboard.press('Escape');
  await expect(panel).toHaveCount(0);
};

test.describe('task modal Esc with an open menu', () => {
  test.use({ viewport: { width: 1440, height: 900 } });
  test.setTimeout(90000);

  test('Share menu: first Esc closes the menu, second closes the modal', async ({
    page,
  }) => {
    const panel = await openTaskModal(page);
    await page.getByTestId('task-share-trigger').click();
    const menu = page.getByRole('menu');
    await expect(menu).toHaveAttribute('data-state', 'open');
    await expect(page.locator(OPEN_MENU)).toHaveCount(1);
    await escLayerThenModal(page, panel, 'menu', OPEN_MENU);
  });

  test('status select: first Esc closes the listbox, second closes the modal', async ({
    page,
  }) => {
    const panel = await openTaskModal(page);
    await page.locator('[data-tour="task-form.status"]').click();
    const listbox = page.getByRole('listbox');
    await expect(listbox).toHaveAttribute('data-state', 'open');
    await expect(page.locator(OPEN_LISTBOX)).toHaveCount(1);
    await escLayerThenModal(page, panel, 'listbox', OPEN_LISTBOX);
  });

  test('comment draft survives Esc on the Share menu', async ({ page }) => {
    const panel = await openTaskModal(page);
    const draft = page.locator('.new-form-container footer textarea');
    await draft.fill('half-written comment');
    await page.getByTestId('task-share-trigger').click();
    await expect(page.locator(OPEN_MENU)).toHaveCount(1);
    await page.keyboard.press('Escape');
    expect((await afterEsc(page, 'menu')).modalOpen).toBe(true);
    await expect(page.locator(OPEN_MENU)).toHaveCount(0);
    await expect(panel).toHaveCount(1);
    await expect(draft).toHaveValue('half-written comment');
  });

  test('no menu open: a single Esc closes the modal', async ({ page }) => {
    const panel = await openTaskModal(page);
    await expect(page.locator(`${OPEN_MENU}, ${OPEN_LISTBOX}`)).toHaveCount(0);
    await page.keyboard.press('Escape');
    await expect(panel).toHaveCount(0);
  });
});
