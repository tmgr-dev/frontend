import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const TOKEN = 'tok123';
const invitation = {
  workspace: { id: 1, name: 'Demo' },
  already_member: false,
};

const answer = (route, outcome) =>
  outcome === 'abort'
    ? route.abort('internetdisconnected')
    : route.fulfill(outcome);

async function routeInvitation(
  page,
  {
    info = { json: { data: invitation } },
    accept = { json: { data: {} } },
    gate,
  } = {},
) {
  const calls = { info: [], accept: [] };
  await page.route('**/workspaces/invitations/*/info', async (route) => {
    calls.info.push(new URL(route.request().url()).pathname);
    if (gate) await gate;
    return answer(route, info);
  });
  await page.route('**/workspaces/invitations/*/accept', (route) => {
    calls.accept.push(new URL(route.request().url()).pathname);
    return answer(route, accept);
  });
  return calls;
}

const storedInvitation = (page) =>
  page.evaluate(() => localStorage.getItem('workspace.invitation'));

const joined = (page) =>
  expect(
    page.getByText('You have successfully joined the workspace!'),
  ).toBeVisible();

test('signed-in invitee sees loading, then the Accept button', async ({
  page,
}) => {
  await mockApp(page);
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  const calls = await routeInvitation(page, { gate });
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await expect(page.getByText('Loading invitation details...')).toBeVisible();
  release();
  await expect(
    page.getByRole('button', { name: 'Accept Invitation' }),
  ).toBeVisible();
  await expect(
    page.getByText('You have been invited to join: "Demo"'),
  ).toBeVisible();
  expect(calls.info).toEqual([`/api/workspaces/invitations/${TOKEN}/info`]);
});

test('accepting a token invitation shows joined and clears the stored invite', async ({
  page,
}) => {
  await mockApp(page);
  const calls = await routeInvitation(page);
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await page.getByRole('button', { name: 'Accept Invitation' }).click();
  await joined(page);
  await expect(
    page.getByRole('link', { name: 'Go to workspace' }),
  ).toBeVisible();
  expect(calls.accept).toEqual([`/api/workspaces/invitations/${TOKEN}/accept`]);
  expect(await storedInvitation(page)).toBeNull();
});

test('accepting an id-89 invitation requests and joins that exact token', async ({
  page,
}) => {
  await mockApp(page);
  const calls = await routeInvitation(page);
  await page.goto('/workspaces/invitations/id-89');

  await page.getByRole('button', { name: 'Accept Invitation' }).click();
  await joined(page);
  expect(calls.info).toEqual(['/api/workspaces/invitations/id-89/info']);
  expect(calls.accept).toEqual(['/api/workspaces/invitations/id-89/accept']);
  expect(await storedInvitation(page)).toBeNull();
});

test('already a member shows joined without clicking Accept', async ({
  page,
}) => {
  await mockApp(page);
  const calls = await routeInvitation(page, {
    info: { json: { data: { ...invitation, already_member: true } } },
  });
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await joined(page);
  await expect(
    page.getByRole('button', { name: 'Accept Invitation' }),
  ).toHaveCount(0);
  expect(calls.accept).toEqual([]);
  expect(await storedInvitation(page)).toBeNull();
});

test('email not verified shows the message, Try again reloads, and the invite is kept', async ({
  page,
}) => {
  await mockApp(page);
  const calls = await routeInvitation(page, {
    info: {
      status: 403,
      json: { message: 'Verify your email address to accept this invitation' },
    },
  });
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await expect(
    page.getByText('Verify your email address to accept this invitation'),
  ).toBeVisible();
  await page.getByRole('link', { name: 'Try again' }).click();
  await expect.poll(() => calls.info.length).toBe(2);
  expect(await storedInvitation(page)).toBe(TOKEN);
});

test('invitation for another email shows the correct-email login link and keeps the invite', async ({
  page,
}) => {
  await mockApp(page);
  await routeInvitation(page, {
    info: {
      status: 403,
      json: { message: 'Invitation belongs to another email address' },
    },
  });
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await expect(
    page.getByText('Invitation belongs to another email address'),
  ).toBeVisible();
  await expect(
    page.getByRole('link', { name: 'Login with correct email' }),
  ).toBeVisible();
  expect(await storedInvitation(page)).toBe(TOKEN);
});

test('Login with correct email navigates to /login', async ({ page }) => {
  await mockApp(page);
  await routeInvitation(page, {
    info: {
      status: 403,
      json: { message: 'Invitation belongs to another email address' },
    },
  });
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await page.getByRole('link', { name: 'Login with correct email' }).click();
  await expect(page).toHaveURL(/\/login$/);
});

test('invalid token shows the message and clears the stored invite', async ({
  page,
}) => {
  await mockApp(page);
  await routeInvitation(page, {
    info: { status: 404, json: { message: 'Invalid invitation token' } },
  });
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await expect(page.getByText('Invalid invitation token')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Try again' })).toHaveCount(0);
  expect(await storedInvitation(page)).toBeNull();
});

test('expired invitation shows the message and clears the stored invite', async ({
  page,
}) => {
  await mockApp(page);
  await routeInvitation(page, {
    info: { status: 422, json: { message: 'Invitation expired' } },
  });
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await expect(page.getByText('Invitation expired')).toBeVisible();
  expect(await storedInvitation(page)).toBeNull();
});

for (const [label, info] of [
  ['server error without a message', { status: 500, json: {} }],
  ['network failure', 'abort'],
]) {
  test(`${label} shows the generic load failure and keeps the stored invite`, async ({
    page,
  }) => {
    await mockApp(page);
    await routeInvitation(page, { info });
    await page.goto(`/workspaces/invitations/${TOKEN}`);

    await expect(
      page.getByText('Failed to load invitation details'),
    ).toBeVisible();
    expect(await storedInvitation(page)).toBe(TOKEN);
  });
}

test('guest sees Register and Sign in and the invite is stored', async ({
  page,
}) => {
  await mockApp(page, { account: false });
  const calls = await routeInvitation(page);
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await expect(
    page.getByRole('button', { name: 'Create account' }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
  await expect(
    page.getByText('Log in or create an account to accept this invitation.'),
  ).toBeVisible();
  expect(await storedInvitation(page)).toBe(TOKEN);
  expect(calls.info).toEqual([]);
});

test('guest signs in and returns to the invitation', async ({ page }) => {
  await mockApp(page, { account: false, loginAs: 'tok' });
  await routeInvitation(page);
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(/\/login$/);
  await page.locator('input[type="email"]').fill('test@example.test');
  await page.locator('input[type="password"]').fill('secret-pass');
  await page.locator('form button[type="submit"]').click();

  await expect(page).toHaveURL(new RegExp(`/workspaces/invitations/${TOKEN}$`));
  await expect(
    page.getByRole('button', { name: 'Accept Invitation' }),
  ).toBeVisible();
});

test('guest registers and returns to the invitation', async ({ page }) => {
  await mockApp(page, { account: false, loginAs: 'tok' });
  await routeInvitation(page);
  await page.goto(`/workspaces/invitations/${TOKEN}`);

  await page.getByRole('button', { name: 'Create account' }).click();
  await expect(page).toHaveURL(/\/register$/);
  await page.locator('#name').fill('Test User');
  await page.locator('#email').fill('test@example.test');
  await page.locator('#password').fill('secret-pass');
  await page.locator('#password_confirmation').fill('secret-pass');
  await page.locator('form button[type="submit"]').click();

  await expect(page).toHaveURL(new RegExp(`/workspaces/invitations/${TOKEN}$`));
  await expect(
    page.getByRole('button', { name: 'Accept Invitation' }),
  ).toBeVisible();
});
