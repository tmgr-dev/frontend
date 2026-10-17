import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const shotsDir = process.env.SHOTS_DIR;
const shotsSuffix = process.env.SHOTS_SUFFIX ?? '';

const screenshot = async (page, name) => {
  if (!shotsDir) return;
  await page.screenshot({
    path: `${shotsDir}/connect-agent-${name}${shotsSuffix}.png`,
  });
};

const persona = {
  id: 'uuid-1',
  name: 'Reviewer',
  description: 'Reviews incoming tasks',
  avatar_url: null,
  archived_at: null,
  owner: { id: 1, name: 'Test User' },
  system_prompt: 'You are a reviewer.',
  prompt_version: 1,
};

const openIssuedDialog = async (page) => {
  await desktopPage(page);
  // The desktop.move-to-cloud tour offer fires once a local workspace exists
  // and navigates to Settings → Local data mid-test.
  await page.addInitScript(() =>
    localStorage.setItem('tmgr:tours:offered:desktop.move-to-cloud', '1'),
  );
  // First-visit tours auto-offer ~600ms after landing and cover the page.
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await page.route('**/api/personas*', (route) =>
    route.fulfill({ json: { data: [persona] } }),
  );

  await page.goto('/demo/board');
  await page.getByTitle('Switch workspace').first().click();
  await page.getByRole('menuitem', { name: /New local workspace/ }).click();
  await page.getByPlaceholder(/Personal, Client/).fill('Personal');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/local-personal\//);

  await page.goto('/settings/personas');
  await expect(
    page.getByRole('heading', { name: 'On this device' }),
  ).toBeVisible();
  const personasSection = page.locator('section', {
    hasText: 'Personas in this workspace',
  });
  await personasSection.getByRole('button', { name: 'Sync now' }).click();
  await expect(personasSection.getByText('Reviewer')).toBeVisible();

  await personasSection.getByRole('switch').click();
  await personasSection
    .getByRole('button', { name: 'Connect an agent' })
    .click();
  await expect(
    page.getByRole('heading', { name: 'Connect an agent' }),
  ).toBeVisible();
  await page.getByPlaceholder('Claude Code on my laptop').fill('Claude Code');
  await page.getByRole('button', { name: 'Issue token' }).click();
  await expect(page.getByText('Token issued.')).toBeVisible();
};

// The dialog is centred, so any late reflow (font swap, re-render) shifts it;
// read every box in one frame so dialog and children are measured together.
const measure = (page) =>
  page.getByRole('dialog').evaluate((dialog) => {
    const box = (el) => {
      const r = el.getBoundingClientRect();
      return { x: r.x, y: r.y, width: r.width, height: r.height };
    };
    const done = [...dialog.querySelectorAll('button')].find(
      (b) => b.textContent.trim() === 'Done',
    );
    return {
      dialog: box(dialog),
      pres: [...dialog.querySelectorAll('pre')].map(box),
      done: box(done),
    };
  });

const expectInside = (inner, outer) => {
  expect(inner.x).toBeGreaterThanOrEqual(outer.x - 1);
  expect(inner.y).toBeGreaterThanOrEqual(outer.y - 1);
  expect(inner.x + inner.width).toBeLessThanOrEqual(outer.x + outer.width + 1);
  expect(inner.y + inner.height).toBeLessThanOrEqual(
    outer.y + outer.height + 1,
  );
};

test('the connect-an-agent dialog keeps its issued-state content inside the dialog on a wide window', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1600, height: 1000 });
  await openIssuedDialog(page);

  await screenshot(page, 'wide-light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await screenshot(page, 'wide-dark');
  await page.emulateMedia({ colorScheme: 'light' });

  await expect(page.getByRole('dialog').locator('pre')).toHaveCount(2);
  const boxes = await measure(page);
  for (const pre of boxes.pres) {
    expectInside(pre, boxes.dialog);
  }
  expectInside(boxes.done, boxes.dialog);
});

test('the connect-an-agent dialog stays inside the viewport at phone width', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1600, height: 1000 });
  await openIssuedDialog(page);
  await page.setViewportSize({ width: 375, height: 812 });

  await screenshot(page, 'phone-light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await screenshot(page, 'phone-dark');
  await page.emulateMedia({ colorScheme: 'light' });

  const boxes = await measure(page);
  const viewport = page.viewportSize();
  expectInside(boxes.dialog, {
    x: 0,
    y: 0,
    width: viewport.width,
    height: viewport.height + 10000,
  });
  expectInside(boxes.done, boxes.dialog);
});
