import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const today = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(
    2,
    '0',
  )}-${String(d.getDate()).padStart(2, '0')}`;
};

const routine = (over = {}) => ({
  task_id: 31,
  title: 'Stretch',
  description: null,
  routine_category: { id: 'health', name: 'Health', color: '#22c55e' },
  date: today(),
  time: '08:15',
  duration_min: 60,
  reminder_min: 15,
  completed: false,
  status: 'pending',
  instance_id: null,
  virtual: true,
  frequency: 'DAILY',
  scheduled_for: null,
  ...over,
});

const mockRoutines = async (
  page,
  { entries = () => [routine()], view = 'list' } = {},
) => {
  const api = { putStatus: 200, deleteStatus: 200, putGate: null, writes: [] };
  await page.addInitScript((v) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr.dailyRoutines.view', v);
  }, view);
  await page.route('**/api/daily-routines/**', async (route) => {
    const request = route.request();
    const method = request.method();
    const path = new URL(request.url()).pathname.replace(
      /^.*\/api\/daily-routines\//,
      '',
    );
    if (method !== 'GET')
      api.writes.push({ method, path, body: request.postDataJSON?.() });
    if (method === 'POST' && path === 'tasks/quick')
      return route.fulfill({ json: { data: { id: 40, title: 'Brand new' } } });
    if (method === 'POST' && path.endsWith('/complete-on'))
      return route.fulfill({
        json: {
          data: {
            instance_id: 900,
            task_id: 31,
            date: today(),
            status: 'completed',
            completed: true,
          },
        },
      });
    if (method === 'PUT' && /^tasks\/\d+$/.test(path)) {
      if (api.putGate) await api.putGate;
      return api.putStatus === 200
        ? route.fulfill({ json: { data: { id: Number(path.split('/')[1]) } } })
        : route.fulfill({
            status: api.putStatus,
            json: { message: 'Save failed' },
          });
    }
    if (method === 'DELETE' && /^tasks\/\d+$/.test(path))
      return api.deleteStatus === 200
        ? route.fulfill({ json: { data: null } })
        : route.fulfill({
            status: api.deleteStatus,
            json: { message: 'Delete failed' },
          });
    if (path.startsWith('expand/stats'))
      return route.fulfill({ json: { data: {} } });
    if (path.startsWith('expand')) {
      const params = new URL(request.url()).searchParams;
      return route.fulfill({
        json: { data: entries(params.get('from'), params.get('to')) },
      });
    }
    if (/^tasks\/\d+$/.test(path))
      return route.fulfill({
        json: {
          data: {
            id: 31,
            title: 'Stretch',
            description: 'Neck too',
            routine_category: 'health',
            approximately_time: 20,
            scheduled_date: null,
            scheduled_time: null,
            recurring: false,
          },
        },
      });
    return route.fulfill({ json: { data: [] } });
  });
  return api;
};

const rowButton = (page) => page.getByRole('button', { name: /^Stretch/ });
const editorTitle = (page) => page.getByLabel('Title', { exact: true });
const editorHeading = (page) => page.getByText('Edit routine', { exact: true });
const alertWith = (page, text) =>
  page.getByRole('alert').filter({ hasText: text });
const deletes = (api) => api.writes.filter((w) => w.method === 'DELETE');

const openEditor = async (page) => {
  await page.goto('/routines');
  await rowButton(page).click();
  await expect(editorHeading(page)).toBeVisible();
};

test('a failed save keeps the editor and the edits, shows the error inline, and Save is disabled while in flight', async ({
  page,
}) => {
  await mockApp(page);
  const api = await mockRoutines(page);
  await openEditor(page);
  await editorTitle(page).fill('Stretch deep');
  const save = page.getByRole('button', { name: 'Save changes' });

  api.putStatus = 500;
  let release;
  api.putGate = new Promise((r) => (release = r));
  await save.click();
  await expect(save).toBeDisabled();
  release();

  await expect(alertWith(page, 'Save failed')).toBeVisible();
  await expect(editorHeading(page)).toBeVisible();
  await expect(editorTitle(page)).toHaveValue('Stretch deep');
  await expect(save).toBeEnabled();

  api.putStatus = 200;
  api.putGate = null;
  await save.click();
  await expect(editorHeading(page)).toBeHidden();
  expect(api.writes.filter((w) => w.method === 'PUT')).toHaveLength(2);
});

test('closing the editor is ignored while a save is in flight, so the failure still shows', async ({
  page,
}) => {
  await mockApp(page);
  const api = await mockRoutines(page);
  await openEditor(page);
  await editorTitle(page).fill('Stretch deep');
  api.putStatus = 500;
  let release;
  api.putGate = new Promise((r) => (release = r));
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect(
    page.getByRole('button', { name: 'Save changes' }),
  ).toBeDisabled();

  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.keyboard.press('Escape');
  await page.mouse.click(5, 5);
  await expect(editorHeading(page)).toBeVisible();

  release();
  await expect(alertWith(page, 'Save failed')).toBeVisible();
  await expect(editorTitle(page)).toHaveValue('Stretch deep');
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(editorHeading(page)).toBeHidden();
});

test('press-and-hold on a row title starts a drag, as on the rest of the row', async ({
  page,
}) => {
  await mockApp(page);
  await mockRoutines(page, {
    entries: () => [routine({ task_id: 33, title: 'Read', time: null })],
  });
  await page.goto('/routines');
  const title = page.getByRole('button', { name: /^Read/ });
  await expect(title).toBeVisible();
  const box = await title.boundingBox();
  await page.mouse.move(box.x + 20, box.y + box.height / 2);
  await page.mouse.down();
  const ghost = page.locator('body > div[style*="99999"]');
  await expect(ghost).toHaveCount(1);
  await page.mouse.up();
  await expect(ghost).toHaveCount(0);
  await expect(editorHeading(page)).toBeHidden();
});

test('a new routine whose follow-up PUT fails retries the PUT on the created id, not a second create', async ({
  page,
}) => {
  await mockApp(page);
  const api = await mockRoutines(page);
  await page.goto('/routines');
  await page.getByRole('button', { name: 'New routine' }).click();
  await editorTitle(page).fill('Brand new');
  api.putStatus = 500;
  await page.getByRole('button', { name: 'Create routine' }).click();

  await expect(alertWith(page, 'Save failed')).toBeVisible();
  await expect(editorTitle(page)).toHaveValue('Brand new');

  api.putStatus = 200;
  await page.getByRole('button', { name: 'Create routine' }).click();
  await expect(editorTitle(page)).toBeHidden();

  expect(api.writes.filter((w) => w.path === 'tasks/quick')).toHaveLength(1);
  const puts = api.writes.filter((w) => w.method === 'PUT');
  expect(puts.map((p) => p.path)).toEqual(['tasks/40', 'tasks/40']);
});

test('the modal trash asks first; Cancel sends nothing, Esc closes only the confirm, Confirm deletes', async ({
  page,
}) => {
  await mockApp(page);
  const api = await mockRoutines(page);
  await openEditor(page);
  const trash = page.getByRole('button', { name: 'Delete routine' });
  const confirm = page.getByRole('alertdialog');

  await trash.click();
  await expect(confirm).toContainText('Delete routine?');
  await confirm.getByRole('button', { name: 'Cancel' }).click();
  await expect(confirm).toBeHidden();
  await expect(editorHeading(page)).toBeVisible();

  await trash.click();
  await expect(confirm).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(confirm).toBeHidden();
  await expect(editorHeading(page)).toBeVisible();
  expect(deletes(api)).toEqual([]);

  await trash.click();
  await confirm.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(editorHeading(page)).toBeHidden();
  expect(deletes(api).map((d) => d.path)).toEqual(['tasks/31']);
});

test('a failed delete from the modal keeps the editor open with the error inline', async ({
  page,
}) => {
  await mockApp(page);
  const api = await mockRoutines(page);
  api.deleteStatus = 500;
  await openEditor(page);
  await page.getByRole('button', { name: 'Delete routine' }).click();
  await page
    .getByRole('alertdialog')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();

  await expect(alertWith(page, 'Delete failed')).toBeVisible();
  await expect(editorHeading(page)).toBeVisible();
  await expect(editorTitle(page)).toHaveValue('Stretch');
});

test('the context-menu Delete asks first and only Confirm sends DELETE', async ({
  page,
}) => {
  await mockApp(page);
  const api = await mockRoutines(page);
  let nativeDialogs = 0;
  page.on('dialog', (d) => {
    nativeDialogs += 1;
    d.dismiss();
  });
  await page.goto('/routines');
  const confirm = page.getByRole('alertdialog');

  await rowButton(page).click({ button: 'right' });
  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(confirm).toContainText('"Stretch"');
  await confirm.getByRole('button', { name: 'Cancel' }).click();
  await expect(confirm).toBeHidden();
  expect(deletes(api)).toEqual([]);

  await rowButton(page).click({ button: 'right' });
  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  await confirm.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect.poll(() => deletes(api).length).toBe(1);
  expect(nativeDialogs).toBe(0);
});

test('the confirm opened over the editor is clickable and the editor stays behind it', async ({
  page,
}) => {
  await mockApp(page);
  const api = await mockRoutines(page);
  await openEditor(page);
  await page.getByRole('button', { name: 'Delete routine' }).click();
  const confirm = page.getByRole('alertdialog');
  const top = await confirm.evaluate((el) => {
    const r = el.getBoundingClientRect();
    const hit = document.elementFromPoint(
      r.left + r.width / 2,
      r.top + r.height / 2,
    );
    return el.contains(hit);
  });
  expect(top).toBe(true);
  await confirm.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect.poll(() => deletes(api).length).toBe(1);
});

test('done toggle and icon buttons have names; aria-pressed follows completion', async ({
  page,
}) => {
  await mockApp(page);
  await mockRoutines(page);
  await page.goto('/routines');
  const toggle = page.getByRole('button', { name: 'Mark "Stretch" done' });
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  await expect(toggle).toHaveAttribute('data-tour', 'routines.instance');
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');

  await expect(page.getByLabel('Quick add routine')).toBeVisible();
  await rowButton(page).click();
  await expect(
    page.getByRole('button', { name: 'Close', exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Delete routine' }),
  ).toBeVisible();
  await expect(editorTitle(page)).toBeVisible();
  await expect(
    page.getByRole('textbox', { name: 'Description' }),
  ).toBeVisible();
});

test('day view: prev/next period and the event edit button are named', async ({
  page,
}) => {
  await mockApp(page);
  await mockRoutines(page, { view: 'day' });
  await page.goto('/routines');
  await expect(
    page.getByRole('button', { name: 'Previous period' }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Next period' })).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Edit "Stretch"' }),
  ).toBeVisible();
});

test('Tab reaches a row; Enter and Space open it; Enter on the done toggle toggles only', async ({
  page,
}) => {
  await mockApp(page);
  await mockRoutines(page);
  await page.goto('/routines');
  const row = rowButton(page);
  await expect(row).toBeVisible();

  await page.getByLabel('Quick add routine').focus();
  let reached = false;
  for (let i = 0; i < 25 && !reached; i += 1) {
    await page.keyboard.press('Tab');
    reached = await row.evaluate((el) => el === document.activeElement);
  }
  expect(reached).toBe(true);

  await page.keyboard.press('Enter');
  await expect(editorHeading(page)).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(editorHeading(page)).toBeHidden();

  await row.focus();
  await page.keyboard.press('Space');
  await expect(editorHeading(page)).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(editorHeading(page)).toBeHidden();

  const toggle = page.getByRole('button', { name: 'Mark "Stretch" done' });
  await toggle.focus();
  await page.keyboard.press('Enter');
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
  await expect(editorHeading(page)).toBeHidden();
});

const overlapping = () => [
  routine(),
  routine({ task_id: 32, title: 'Journal', time: '08:30' }),
];

test('cluster popover: toggle and edit are named, the pencil is reachable by keyboard', async ({
  page,
}) => {
  await mockApp(page);
  await mockRoutines(page, { view: 'day', entries: overlapping });
  await page.goto('/routines');
  await page.getByText('2 events').click();

  const toggle = page.getByRole('button', { name: 'Mark "Stretch" done' });
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  const edit = page.getByRole('button', { name: 'Edit "Stretch"' });
  await expect(edit).toHaveCount(1);
  await expect(page.getByRole('button', { name: 'Close' })).toBeVisible();

  await page.mouse.move(0, 0);
  await expect(edit).toHaveCSS('opacity', '0');
  await edit.focus();
  await expect(edit).toHaveCSS('opacity', '1');
});

test('on a pointer device the resize handles stay hidden until hover', async ({
  page,
}) => {
  await mockApp(page);
  await mockRoutines(page, { view: 'day' });
  await page.goto('/routines');
  const handle = page.locator('.dr-resize-handle').first();
  await expect(handle).toHaveCount(1);
  await page.mouse.move(0, 0);
  await expect(handle).toHaveCSS('opacity', '0');
});

test.describe('touch device', () => {
  test.use({ hasTouch: true });

  test('cluster pencil and day-view resize handles are visible without hover', async ({
    page,
  }) => {
    await mockApp(page);
    await mockRoutines(page, { view: 'day', entries: overlapping });
    await page.goto('/routines');
    await page.getByText('2 events').click();
    await expect(
      page.getByRole('button', { name: 'Edit "Stretch"' }),
    ).toHaveCSS('opacity', '1');
  });

  test('resize handles are visible on a single event', async ({ page }) => {
    await mockApp(page);
    await mockRoutines(page, { view: 'day' });
    await page.goto('/routines');
    const handle = page.locator('.dr-resize-handle').first();
    await expect(handle).toHaveCSS('opacity', '1');
  });
});
