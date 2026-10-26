import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

// TM-738: the reaction picker opens above the "Add reaction" button (flips below when there is
// no room), stays inside the comments scroller and the viewport, closes on an outside click and
// on Esc without closing the task modal, and is keyboard operable.

const comment = (id, extra = {}) => ({
  id,
  message: `Comment ${id}\nsecond line of comment ${id}`,
  user: { id: 1, name: 'Test User' },
  created_at: new Date(Date.UTC(2026, 8, 13, 10, 0, id)).toISOString(),
  updated_at: '2026-09-13T10:00:00Z',
  reactions: [],
  ...extra,
});

const thread = () =>
  Array.from({ length: 20 }, (_, i) =>
    i === 5
      ? comment(i + 1, {
          reactions: [
            {
              emoji: '👍',
              count: 1,
              reacted: false,
              users: [{ id: 2, name: 'Ann' }],
            },
          ],
        })
      : comment(i + 1),
  );

const setup = async (page) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page);
  const toggled = [];
  await page.route('**/api/tasks/1/comments**', (route) =>
    route.fulfill({ json: { data: thread() } }),
  );
  await page.route('**/api/comments/*/reactions/toggle', (route) => {
    const { emoji } = route.request().postDataJSON();
    toggled.push(emoji);
    return route.fulfill({
      json: {
        data: [
          {
            emoji,
            count: 1,
            reacted: true,
            users: [{ id: 1, name: 'Test User' }],
          },
        ],
      },
    });
  });
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const form = page.locator('.new-form-container');
  await expect(form.locator('[data-comment-id]')).toHaveCount(20);
  return { form, main: form.locator('main'), toggled };
};

const row = (form, id) => form.locator(`[data-comment-id="${id}"]`);
const addButton = (form, id) =>
  row(form, id).getByRole('button', { name: 'Add reaction' });
const box = (loc) => loc.evaluate((el) => el.getBoundingClientRect().toJSON());

const openPicker = async (page, form, id) => {
  // Raw mouse: locator actions re-scroll the comment, which defeats the scroll position
  // the flip test set up.
  const { x, y, width, height } = await box(addButton(form, id));
  await page.mouse.move(x + width / 2, y + height / 2);
  await page.mouse.click(x + width / 2, y + height / 2);
  const picker = page.getByTestId('reaction-picker');
  await expect(picker).toBeVisible();
  return picker;
};

test.describe('Comment reaction picker', () => {
  test('opens above the button inside the scroller', async ({ page }) => {
    const { form, main } = await setup(page);
    await row(form, 7).scrollIntoViewIfNeeded();
    const picker = await openPicker(page, form, 7);
    const pickerBox = await box(picker);
    const buttonBox = await box(addButton(form, 7));
    const mainBox = await box(main);
    expect(pickerBox.bottom).toBeLessThanOrEqual(buttonBox.top);
    expect(pickerBox.top).toBeGreaterThanOrEqual(mainBox.top);
    expect(pickerBox.left).toBeGreaterThanOrEqual(mainBox.left);
    expect(pickerBox.right).toBeLessThanOrEqual(mainBox.right);
    await expect(picker.getByRole('menuitem')).toHaveCount(6);
  });

  test('flips below the button when there is no room above', async ({
    page,
  }) => {
    const { form, main } = await setup(page);
    await row(form, 1).evaluate((el) => el.scrollIntoView({ block: 'start' }));
    const gap = (await box(addButton(form, 1))).top - (await box(main)).top;
    await main.evaluate((el, by) => (el.scrollTop += by), gap - 12);
    const picker = await openPicker(page, form, 1);
    const pickerBox = await box(picker);
    const buttonBox = await box(addButton(form, 1));
    const mainBox = await box(main);
    expect(pickerBox.top).toBeGreaterThanOrEqual(buttonBox.bottom);
    expect(pickerBox.top).toBeGreaterThanOrEqual(mainBox.top);
    expect(pickerBox.bottom).toBeLessThanOrEqual(mainBox.bottom);
    const viewport = page.viewportSize();
    expect(pickerBox.top).toBeGreaterThanOrEqual(0);
    expect(pickerBox.bottom).toBeLessThanOrEqual(viewport.height);
  });

  test('outside click closes it, the modal stays open', async ({ page }) => {
    const { form } = await setup(page);
    await row(form, 7).scrollIntoViewIfNeeded();
    const picker = await openPicker(page, form, 7);
    await row(form, 8).getByText('Comment 8').click();
    await expect(picker).toHaveCount(0);
    await expect(form).toBeVisible();
  });

  test('a click on the modal backdrop closes only the picker', async ({
    page,
  }) => {
    const { form } = await setup(page);
    await row(form, 7).scrollIntoViewIfNeeded();
    const picker = await openPicker(page, form, 7);
    await page.mouse.click(4, 4);
    await expect(picker).toHaveCount(0);
    await page.waitForTimeout(600);
    await expect(form).toBeVisible();
  });

  test('Esc closes only the picker, a second Esc closes the modal', async ({
    page,
  }) => {
    const { form } = await setup(page);
    await row(form, 7).scrollIntoViewIfNeeded();
    const picker = await openPicker(page, form, 7);
    await expect(picker.getByRole('menuitem').first()).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(picker).toHaveCount(0);
    await expect(addButton(form, 7)).toBeFocused();
    await page.waitForTimeout(400);
    await expect(form).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(form).toHaveCount(0);
  });

  test('Esc closes the picker when focus has left it', async ({ page }) => {
    const { form } = await setup(page);
    await row(form, 7).scrollIntoViewIfNeeded();
    const picker = await openPicker(page, form, 7);
    const { x, y, height } = await box(picker);
    await page.mouse.click(x + 2, y + height / 2);
    await expect(picker.getByRole('menuitem').first()).not.toBeFocused();
    await expect(picker).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(picker).toHaveCount(0);
    await page.waitForTimeout(400);
    await expect(form).toBeVisible();
  });

  test('keyboard: Tab to the button, Enter opens, arrows move, Enter picks', async ({
    page,
  }) => {
    const { form, toggled } = await setup(page);
    const add = addButton(form, 6);
    await row(form, 6).scrollIntoViewIfNeeded();
    await row(form, 6).getByRole('button', { name: /👍/ }).focus();
    await page.keyboard.press('Tab');
    await expect(add).toBeFocused();
    await expect(add).toHaveCSS('opacity', '1');

    await page.keyboard.press('Enter');
    const picker = page.getByTestId('reaction-picker');
    const items = picker.getByRole('menuitem');
    await expect(items.first()).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await expect(items.nth(1)).toBeFocused();
    await page.keyboard.press('ArrowLeft');
    await expect(items.first()).toBeFocused();
    await page.keyboard.press('ArrowLeft');
    await expect(items.last()).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(items.first()).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(items.nth(1)).toBeFocused();
    await page.keyboard.press('Enter');

    await expect(picker).toHaveCount(0);
    await expect(add).toBeFocused();
    await expect(
      row(form, 6).getByRole('button', { name: /❤️/ }),
    ).toBeVisible();
    expect(toggled).toEqual(['❤️']);
  });
});
