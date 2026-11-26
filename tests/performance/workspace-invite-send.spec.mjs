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

async function setup(page, { workspaces = [ws()] } = {}) {
  await quietTours(page);
  await mockApp(page);
  const state = {
    workspaces,
    invites: [],
    invitePaths: [],
    inviteStatus: 201,
    inviteJson: { data: {} },
  };
  await page.route('**/api/workspaces', (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    return route.fulfill({ json: { data: state.workspaces } });
  });
  await page.route('**/api/workspaces/1/invitations', (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    state.invitePaths.push(new URL(route.request().url()).pathname);
    state.invites.push(route.request().postDataJSON());
    return route.fulfill({
      status: state.inviteStatus,
      json: state.inviteJson,
    });
  });
  return state;
}

const inviteButton = (page) =>
  page.getByRole('button', { name: 'Invite', exact: true });
const dialog = (page) =>
  page.getByRole('dialog', { name: 'Send invitation(s)' });
const emailsBox = (page) => dialog(page).getByRole('textbox');
const sendButton = (page) => dialog(page).getByRole('button', { name: 'Send' });

test('inviting one valid email POSTs it to the workspace and closes the modal', async ({
  page,
}) => {
  const state = await setup(page);
  await page.goto('/settings/workspaces');
  await inviteButton(page).click();
  await expect(dialog(page)).toBeVisible();

  await emailsBox(page).fill('a@example.test');
  await expect(sendButton(page)).toBeEnabled();
  await sendButton(page).click();

  await expect(dialog(page)).toBeHidden();
  expect(state.invitePaths).toEqual(['/api/workspaces/1/invitations']);
  expect(state.invites).toEqual([{ emails: 'a@example.test' }]);
});

test('comma-separated emails are sent as one string', async ({ page }) => {
  const state = await setup(page);
  await page.goto('/settings/workspaces');
  await inviteButton(page).click();

  await emailsBox(page).fill('a@example.test, b@example.test');
  await sendButton(page).click();

  await expect(dialog(page)).toBeHidden();
  expect(state.invites).toEqual([{ emails: 'a@example.test, b@example.test' }]);
});

test('an invalid email shows the validation error and sends nothing', async ({
  page,
}) => {
  const state = await setup(page);
  await page.goto('/settings/workspaces');
  await inviteButton(page).click();

  await emailsBox(page).fill('not-an-email');
  await expect(
    dialog(page).getByText('Invalid email format: not-an-email', {
      exact: true,
    }),
  ).toBeVisible();
  await expect(sendButton(page)).toBeDisabled();
  expect(state.invites).toEqual([]);
  await expect(dialog(page)).toBeVisible();
});

test('a 403 with a verification message shows it in an error toast and keeps the modal open', async ({
  page,
}) => {
  const state = await setup(page);
  state.inviteStatus = 403;
  state.inviteJson = {
    message: 'Verify your email address before inviting people',
  };
  await page.goto('/settings/workspaces');
  await inviteButton(page).click();

  await emailsBox(page).fill('a@example.test');
  await sendButton(page).click();

  await expect.poll(() => state.invites.length).toBe(1);
  const toast = page.locator('[role="status"], [role="alert"], li').filter({
    hasText: 'Verify your email address before inviting people',
  });
  await expect(toast.first()).toBeVisible();
  await expect(toast.first()).toContainText('Error');
  await expect(dialog(page)).toBeVisible();
  await expect(emailsBox(page)).toHaveValue('a@example.test');
});

test('the members-list invite dialog on the board sends invitations for the workspace', async ({
  page,
}) => {
  await quietTours(page);
  await desktopPage(page);
  const state = await setup(page);
  await page.goto('/demo/board');

  const boardInvite = page.getByRole('button', { name: 'Invite user' }).first();
  await expect(boardInvite).toBeVisible();
  await boardInvite.click();
  await emailsBox(page).fill('a@example.test');
  await sendButton(page).click();

  await expect(dialog(page)).toBeHidden();
  expect(state.invites).toEqual([{ emails: 'a@example.test' }]);
});
