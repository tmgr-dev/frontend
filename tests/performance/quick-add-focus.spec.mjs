import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

// TM-735: every Quick add field shows keyboard focus (outline or a border change) and has an accessible name.
// QUICK_ADD_SHOTS_DIR=<abs dir> also saves a screenshot of a focused field.
const SHOTS = process.env.QUICK_ADD_SHOTS_DIR;

const PAGES = [
  {
    id: 1,
    title: 'Notes',
    slug: 'notes',
    type: 'page',
    parent_id: null,
    position: 0,
    pinned: false,
    updated_at: '2026-10-01T10:00:00Z',
  },
];

const setup = async (page, scheme) => {
  await desktopPage(page, {}, { windowLabel: 'quick-add' });
  await page.addInitScript((scheme) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
    if (scheme === 'dark') localStorage.setItem('colorScheme', 'dark');
    const internals = window.__TAURI_INTERNALS__;
    const base = internals.invoke;
    const queue = [{ workspaceId: 1 }];
    internals.invoke = (command, args, options) => {
      if (command === 'take_quick_add')
        return Promise.resolve(queue.shift() ?? null);
      if (command === 'hide_quick_add') return Promise.resolve(null);
      return base(command, args, options);
    };
  }, scheme);
  await page.route('**/api/**', async (route) => {
    const path = new URL(route.request().url()).pathname.replace('/api/', '');
    if (route.request().method() !== 'GET') return route.fallback();
    if (path === 'project_categories')
      return route.fulfill({ json: { data: [] } });
    if (path === 'pages/tree') return route.fulfill({ json: { data: PAGES } });
    if (path === 'pages/1')
      return route.fulfill({
        json: { data: { ...PAGES[0], body: '## Ideas\n\ntext\n' } },
      });
    return route.fallback();
  });
};

const keyboardFocus = async (
  page,
  locator,
  [away, back] = ['Tab', 'Shift+Tab'],
) => {
  await locator.focus();
  await page.keyboard.press(away);
  await page.keyboard.press(back);
  await expect(locator).toBeFocused();
  return locator.evaluate(async (el) => {
    const look = () => {
      const cs = getComputedStyle(el);
      return {
        outline:
          cs.outlineStyle !== 'none' && parseFloat(cs.outlineWidth) > 0
            ? `${cs.outlineStyle} ${cs.outlineWidth} ${cs.outlineColor}`
            : null,
        sides: ['Top', 'Right', 'Bottom', 'Left'].map((side) => ({
          width: parseFloat(cs[`border${side}Width`]),
          color: cs[`border${side}Color`],
        })),
      };
    };
    const settle = () =>
      new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    await settle();
    await Promise.all(
      el.getAnimations().map((a) => a.finished.catch(() => {})),
    );
    const focused = look();
    const visible = el.matches(':focus-visible');
    el.blur();
    await settle();
    await Promise.all(
      el.getAnimations().map((a) => a.finished.catch(() => {})),
    );
    const blurred = look();
    el.focus();
    return {
      visible,
      outline: focused.outline,
      borderChanged: focused.sides.some(
        (side, i) => side.width > 0 && side.color !== blurred.sides[i].color,
      ),
    };
  });
};

for (const scheme of ['light', 'dark']) {
  test(`quick add ${scheme}: every field shows keyboard focus and has a name`, async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await setup(page, scheme);
    await page.setViewportSize({ width: 640, height: 420 });
    await page.goto('/quick-add');
    await expect(page.getByTestId('quick-add-mode-task')).toBeVisible();
    await expect(
      page.locator('select option', { hasText: 'Demo' }),
    ).toHaveCount(1);

    const fields = [];
    const check = async (name, locator, label, keys) => {
      await expect(locator, name).toBeVisible();
      await expect(locator, `${name} accessible name`).toHaveAccessibleName(
        label,
      );
      const look = await keyboardFocus(page, locator, keys);
      fields.push({ name, ...look });
      return look;
    };

    await page.getByTestId('quick-add-mode-task').click();
    await check('workspace', page.getByLabel('Workspace'), 'Workspace');
    await check('category', page.getByTestId('quick-add-category'), 'Category');
    await check(
      'title',
      page.getByPlaceholder('What needs to be done?'),
      'Title',
    );
    await page.getByRole('button', { name: '+ Description' }).click();
    await check(
      'description',
      page.getByPlaceholder('Description'),
      'Description',
    );
    if (SHOTS)
      await page.screenshot({
        path: `${SHOTS}/quick-add-${scheme}-description.png`,
      });
    await keyboardFocus(page, page.getByPlaceholder('What needs to be done?'));
    if (SHOTS)
      await page.screenshot({ path: `${SHOTS}/quick-add-${scheme}-title.png` });

    await page.getByTestId('quick-add-mode-page').click();
    await expect(page.getByTestId('quick-add-page')).toBeEnabled();
    await page.getByTestId('quick-add-page').selectOption({ label: 'Notes' });
    await expect(page.getByTestId('quick-add-section')).toBeEnabled();
    await check('find page', page.getByPlaceholder('Find page'), 'Find page');
    await check('page', page.getByTestId('quick-add-page'), 'Page');
    await check('section', page.getByTestId('quick-add-section'), 'Section');
    await check(
      'page text',
      page.getByPlaceholder('What to add to the page?'),
      'Text to add to the page',
      ['Shift+Tab', 'Tab'],
    );
    if (SHOTS)
      await page.screenshot({
        path: `${SHOTS}/quick-add-${scheme}-page-text.png`,
      });

    for (const f of fields) {
      expect(f.visible, `${f.name}: focus matches :focus-visible`).toBe(true);
      expect(
        Boolean(f.outline) || f.borderChanged,
        `${f.name}: keyboard focus draws an outline or changes a visible (>0px) border`,
      ).toBe(true);
    }
  });
}
