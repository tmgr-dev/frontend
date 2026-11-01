import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { desktopPage } from './desktopShell.mjs';
import { mockApp } from './mockApp.mjs';

const screensDir = process.env.SCREENS_DIR;
if (screensDir) mkdirSync(screensDir, { recursive: true });
const shot = async (page, name) => {
  await page.waitForTimeout(500);
  if (screensDir) await page.screenshot({ path: join(screensDir, name) });
};

const ORIGINAL = 'fix login bug';
const baseTask = (description = ORIGINAL) => ({
  id: 1,
  title: 'Original task',
  description,
  description_json: null,
  status: 'created',
  status_id: 1,
  workspace_id: 1,
  user_id: 1,
  user: { id: 1, name: 'Test User', email: 'test@example.test' },
  assignees: [],
  settings: [],
  checkpoints: [],
  common_time: 0,
  start_time: 0,
  approximately_time: 0,
  project_category_id: null,
  created_at: '2026-09-13T10:00:00Z',
  updated_at: '2026-09-13T10:00:00Z',
});

const setup = async (
  page,
  { description = ORIGINAL, scheme = 'light', aiOff = false, replies } = {},
) => {
  await page.addInitScript((colorScheme) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('preferred_editor', 'markdown');
    localStorage.setItem('colorScheme', colorScheme);
  }, scheme);
  await mockApp(page);
  await page.route('**/api/workspaces/*/modules', (route) =>
    route.fulfill({
      json: {
        data: {
          configured: true,
          modules: aiOff ? [{ key: 'ai.assistant', enabled: false }] : [],
        },
      },
    }),
  );
  const saves = [];
  await page.route('**/api/tasks/1', (route) => {
    if (route.request().method() === 'PUT') {
      const payload = route.request().postDataJSON();
      saves.push(payload);
      return route.fulfill({
        json: { data: { ...baseTask(description), ...payload } },
      });
    }
    return route.fulfill({ json: { data: baseTask(description) } });
  });
  await page.route('**/api/tasks/1/comments**', (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  const requests = [];
  let n = 0;
  await page.route('**/api/ai/optimize', async (route) => {
    const text = route.request().postDataJSON().text;
    requests.push(text);
    n += 1;
    const reply = replies ? await replies(n, text) : null;
    if (reply?.status)
      return route.fulfill({
        status: reply.status,
        json: { message: 'fixture' },
      });
    return route.fulfill({
      json: { data: reply?.data ?? `# Rewrite ${n}\n\nBody of rewrite ${n}` },
    });
  });
  return { saves, requests };
};

const openForm = async (page) => {
  await page.goto('/demo/tasks/1');
  const form = page.locator('.new-form-container');
  await expect(form.getByLabel('Task name')).toHaveValue('Original task');
  return form;
};

const dialog = (page) => page.getByRole('dialog', { name: 'Rewrite with AI' });

test('rewrite, refine, switch revisions, accept and undo', async ({ page }) => {
  const { requests } = await setup(page);
  const form = await openForm(page);
  const open = form.getByTestId('ai-rewrite-open');
  await expect(open).toBeEnabled();
  await open.click();

  const d = dialog(page);
  await expect(d.getByTestId('ai-rewrite-preview')).toContainText(
    'Body of rewrite 1',
  );
  await expect(d.getByTestId('ai-rewrite-original')).toContainText(ORIGINAL);
  expect(requests[0]).toBe(ORIGINAL);
  await expect(d.getByRole('tab')).toHaveText(['Original', 'Revision 1']);
  await shot(page, 'light-preview.png');

  await d.getByTestId('ai-rewrite-instruction').fill('make it shorter');
  await d.getByTestId('ai-rewrite-refine').click();
  await expect(d.getByTestId('ai-rewrite-preview')).toContainText(
    'Body of rewrite 2',
  );
  expect(requests[1]).toContain('<description>\n# Rewrite 1');
  expect(requests[1]).toContain(
    '<instruction>\nmake it shorter\n</instruction>',
  );
  await expect(d.getByRole('tab')).toHaveText([
    'Original',
    'Revision 1',
    'Revision 2',
  ]);
  await expect(d.getByTestId('ai-rewrite-instruction')).toHaveValue('');

  await d.getByRole('tab', { name: 'Revision 1' }).click();
  await expect(d.getByTestId('ai-rewrite-preview')).toContainText(
    'Body of rewrite 1',
  );
  await expect(d.getByTestId('ai-rewrite-original')).toContainText(ORIGINAL);

  await d.getByTestId('ai-rewrite-instruction').fill('add a title');
  await d.getByTestId('ai-rewrite-refine').click();
  await expect(d.getByRole('tab')).toHaveCount(4);
  expect(requests[2]).toContain('<description>\n# Rewrite 1');

  await d.getByRole('tab', { name: 'Revision 1' }).click();
  await d.getByTestId('ai-rewrite-accept').click();
  await expect(d).toHaveCount(0);
  const wrapper = form.locator('.description-editor-wrapper');
  await expect(wrapper).toContainText('Body of rewrite 1');
  await expect(wrapper).not.toContainText(ORIGINAL);

  await form.getByTestId('ai-rewrite-undo').click();
  await expect(wrapper).toContainText(ORIGINAL);
  await expect(wrapper).not.toContainText('Body of rewrite');
  await expect(form.getByTestId('ai-rewrite-undo')).toHaveCount(0);
});

test('accepting only changes the form; autosave carries it to the server', async ({
  page,
}) => {
  const { saves } = await setup(page);
  const form = await openForm(page);
  await form.getByTestId('ai-rewrite-open').click();
  await expect(dialog(page).getByTestId('ai-rewrite-preview')).toContainText(
    'Body of rewrite 1',
  );
  expect(saves).toEqual([]);
  await dialog(page).getByTestId('ai-rewrite-accept').click();
  await expect
    .poll(() => saves.map((s) => s.description).filter(Boolean), {
      timeout: 15000,
    })
    .toContain('# Rewrite 1\n\nBody of rewrite 1');
});

test('cancel leaves the description unchanged and saves nothing', async ({
  page,
}) => {
  const { saves } = await setup(page);
  const form = await openForm(page);
  await form.getByTestId('ai-rewrite-open').click();
  await expect(dialog(page).getByTestId('ai-rewrite-preview')).toContainText(
    'Body of rewrite 1',
  );
  await dialog(page).getByTestId('ai-rewrite-cancel').click();
  await expect(dialog(page)).toHaveCount(0);
  await expect(form.locator('.description-editor-wrapper')).toContainText(
    ORIGINAL,
  );
  await expect(form.getByTestId('ai-rewrite-undo')).toHaveCount(0);
  await page.waitForTimeout(2500);
  expect(saves).toEqual([]);
});

test('a response that arrives after cancel is ignored', async ({ page }) => {
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  await setup(page, {
    replies: async (n) => {
      if (n === 1) {
        await gate;
        return { data: 'STALE ANSWER' };
      }
      return { data: 'FRESH ANSWER' };
    },
  });
  const form = await openForm(page);
  await form.getByTestId('ai-rewrite-open').click();
  await expect(dialog(page).getByTestId('ai-rewrite-preview')).toContainText(
    'Rewriting',
  );
  await dialog(page).getByTestId('ai-rewrite-cancel').click();
  await form.getByTestId('ai-rewrite-open').click();
  await expect(dialog(page).getByTestId('ai-rewrite-preview')).toContainText(
    'FRESH ANSWER',
  );
  release();
  await page.waitForTimeout(500);
  await expect(dialog(page)).not.toContainText('STALE ANSWER');
  await expect(dialog(page).getByRole('tab')).toHaveCount(2);
});

test('errors are readable and keep revisions and the typed instruction', async ({
  page,
}) => {
  await setup(page, {
    replies: (n) =>
      n === 2 ? { status: 429 } : n === 3 ? { status: 413 } : {},
  });
  const form = await openForm(page);
  await form.getByTestId('ai-rewrite-open').click();
  const d = dialog(page);
  await expect(d.getByRole('tab')).toHaveCount(2);

  await d.getByTestId('ai-rewrite-instruction').fill('more detail');
  await d.getByTestId('ai-rewrite-refine').click();
  await expect(d.getByTestId('ai-rewrite-error')).toContainText(
    'Too many requests',
  );
  await expect(d.getByRole('tab')).toHaveCount(2);
  await expect(d.getByTestId('ai-rewrite-instruction')).toHaveValue(
    'more detail',
  );
  await shot(page, 'light-error.png');

  await d.getByTestId('ai-rewrite-refine').click();
  await expect(d.getByTestId('ai-rewrite-error')).toContainText('too long');
  await expect(d.getByTestId('ai-rewrite-instruction')).toHaveValue(
    'more detail',
  );

  await d.getByTestId('ai-rewrite-refine').click();
  await expect(d.getByTestId('ai-rewrite-error')).toHaveCount(0);
  await expect(d.getByRole('tab')).toHaveCount(3);
});

test('the button is disabled with a hint when the description is empty', async ({
  page,
}) => {
  await setup(page, { description: '' });
  const form = await openForm(page);
  const open = form.getByTestId('ai-rewrite-open');
  await expect(open).toBeDisabled();
  await expect(open).toHaveAttribute('title', 'Write a description first');
});

test('the button is hidden when the AI assistant module is off', async ({
  page,
}) => {
  await setup(page, { aiOff: true });
  const form = await openForm(page);
  await expect(form.locator('.description-editor-wrapper')).toBeVisible();
  await expect(form.getByTestId('ai-rewrite-open')).toHaveCount(0);
});

test('the button is hidden in a local workspace and nothing is sent to the AI endpoint', async ({
  page,
}) => {
  test.setTimeout(120000);
  await page.addInitScript(() => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1');
  });
  const aiCalls = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname.includes('/ai/')) aiCalls.push(1);
  });
  await desktopPage(page);
  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);
  await page.goto('/local-personal/board');
  await page
    .getByRole('button', { name: 'New task', exact: true })
    .first()
    .click();
  await page.getByPlaceholder('Task name').fill('Offline');
  await expect(page.locator('.description-editor-wrapper')).toBeVisible();
  await expect(page.getByTestId('ai-rewrite-open')).toHaveCount(0);
  expect(aiCalls).toEqual([]);
});

for (const [scheme, width, name] of [
  ['dark', 1280, 'dark'],
  ['light', 360, 'narrow'],
  ['dark', 360, 'narrow-dark'],
]) {
  test(`layout screenshots (${name})`, async ({ page }) => {
    await page.setViewportSize({ width, height: width === 360 ? 740 : 800 });
    await setup(page, { scheme });
    const form = await openForm(page);
    await form.getByTestId('ai-rewrite-open').click();
    await expect(dialog(page).getByTestId('ai-rewrite-preview')).toContainText(
      'Body of rewrite 1',
    );
    await dialog(page).getByTestId('ai-rewrite-instruction').fill('shorter');
    await dialog(page).getByTestId('ai-rewrite-refine').click();
    await expect(dialog(page).getByRole('tab')).toHaveCount(3);
    await shot(page, `${name}-dialog.png`);
    const box = await dialog(page).boundingBox();
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(width);
  });
}
