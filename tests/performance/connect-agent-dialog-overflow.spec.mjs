import { expect, test } from '@playwright/test';
import { desktopPage } from './desktopShell.mjs';

const shotsDir = process.env.SHOTS_DIR;
const shotsSuffix = process.env.SHOTS_SUFFIX ?? '';

const screenshot = async (page, name) => {
  if (!shotsDir) return;
  await page.screenshot({ path: `${shotsDir}/connect-agent-${name}${shotsSuffix}.png` });
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
  await expect(page.getByText('On this device')).toBeVisible();
  const personasSection = page.locator('section', {
    hasText: 'Personas in this workspace',
  });
  await personasSection.getByRole('button', { name: 'Sync now' }).click();
  await expect(personasSection.getByText('Reviewer')).toBeVisible();

  await personasSection.getByRole('switch').click();
  await personasSection.getByRole('button', { name: 'Connect an agent' }).click();
  await expect(
    page.getByRole('heading', { name: 'Connect an agent' }),
  ).toBeVisible();
  await page.getByPlaceholder('Claude Code on my laptop').fill('Claude Code');
  await page.getByRole('button', { name: 'Issue token' }).click();
  await expect(page.getByText('Token issued.')).toBeVisible();
};

const expectInside = (inner, outer) => {
  expect(inner.x).toBeGreaterThanOrEqual(outer.x - 1);
  expect(inner.y).toBeGreaterThanOrEqual(outer.y - 1);
  expect(inner.x + inner.width).toBeLessThanOrEqual(outer.x + outer.width + 1);
  expect(inner.y + inner.height).toBeLessThanOrEqual(outer.y + outer.height + 1);
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

  const dialog = page.getByRole('dialog');
  const dialogBox = await dialog.boundingBox();
  expect(dialogBox).toBeTruthy();

  const pres = dialog.locator('pre');
  await expect(pres).toHaveCount(2);
  for (const pre of await pres.all()) {
    expectInside(await pre.boundingBox(), dialogBox);
  }

  const doneButton = page.getByRole('button', { name: 'Done' });
  expectInside(await doneButton.boundingBox(), dialogBox);
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

  const dialog = page.getByRole('dialog');
  const dialogBox = await dialog.boundingBox();
  const viewport = page.viewportSize();
  expectInside(dialogBox, { x: 0, y: 0, width: viewport.width, height: viewport.height + 10000 });

  const doneButton = page.getByRole('button', { name: 'Done' });
  expectInside(await doneButton.boundingBox(), dialogBox);
});
