import { expect, test } from '@playwright/test';
import { createHash } from 'node:crypto';
import { mockApp } from './mockApp.mjs';

const CODE = 'c'.repeat(43);
const TX = 't'.repeat(43);
const ENVELOPE = {
  data: {
    token: 'desktop-jwt',
    refresh_token: 'desktop-refresh',
    token_type: 'Bearer',
    user: { id: 1, name: 'Test User', email: 'test@example.test' },
  },
};

const s256 = (verifier) =>
  createHash('sha256').update(verifier).digest('base64url');

/** Logged-out page inside a stubbed desktop shell; deep links arrive through `window.__emit`. */
const loggedOutDesktop = async (page) => {
  await mockApp(page);
  await page.addInitScript(() => {
    if (!sessionStorage.getItem('fixture.loggedOut')) {
      sessionStorage.setItem('fixture.loggedOut', '1');
      localStorage.removeItem('token');
    }
    window.__listeners = {};
    window.__emit = (event, payload) =>
      (window.__listeners[event] ?? []).forEach((id) =>
        window[`__callback${id}`]?.({ event, id, payload }),
      );
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      invoke: (command, args) => {
        if (command === 'plugin:event|listen') {
          (window.__listeners[args.event] ??= []).push(args.handler);
          return Promise.resolve(args.handler);
        }
        return Promise.resolve(null);
      },
      transformCallback: (callback) => {
        const id = (window.__callbackId = (window.__callbackId ?? 0) + 1);
        window[`__callback${id}`] = callback;
        return id;
      },
    };
  });
  const starts = [];
  await page.route('**/api/auth/login/desktop/github?*', async (route) => {
    starts.push(new URL(route.request().url()));
    await route.fulfill({ status: 204, body: '' });
  });
  const accepts = [];
  await page.route('**/api/auth/login/desktop/accept', async (route) => {
    accepts.push(route.request().postDataJSON());
    await route.fulfill({ json: ENVELOPE });
  });
  return { starts, accepts };
};

test('desktop: GitHub sign-in opens the browser flow and the deep link signs the app in', async ({
  page,
}) => {
  const { starts, accepts } = await loggedOutDesktop(page);
  await page.goto('/login');

  await page.getByRole('button', { name: 'GitHub' }).click();
  await expect(page.getByText('Continue in your browser')).toBeVisible();
  await expect.poll(() => starts.length).toBe(1);
  const state = starts[0].searchParams.get('state');
  const challenge = starts[0].searchParams.get('code_challenge');

  await page.evaluate(
    (url) => window.__emit('deep-link://new-url', [url]),
    `tmgr://auth/callback?code=${CODE}&state=${'x'.repeat(43)}`,
  );
  await page.waitForTimeout(300);
  expect(accepts).toHaveLength(0);

  await page.evaluate(
    (url) => window.__emit('deep-link://new-url', [url]),
    `tmgr://auth/callback?code=${CODE}&state=${state}`,
  );
  await expect.poll(() => accepts.length).toBe(1);
  expect(accepts[0].code).toBe(CODE);
  expect(s256(accepts[0].code_verifier)).toBe(challenge);
  await expect(page).not.toHaveURL(/\/login/);
  expect(
    await page.evaluate(() => JSON.parse(localStorage.getItem('token')).token),
  ).toBe('desktop-jwt');
});

test('desktop: an uncorrelated error link does not cancel the attempt', async ({
  page,
}) => {
  const { starts, accepts } = await loggedOutDesktop(page);
  await page.goto('/login');
  await page.getByRole('button', { name: 'GitHub' }).click();
  await expect.poll(() => starts.length).toBe(1);
  const state = starts[0].searchParams.get('state');

  await page.evaluate(() =>
    window.__emit('deep-link://new-url', ['tmgr://auth/callback?error=apple']),
  );
  await expect(page.getByText('Sign-in was not completed')).toBeVisible();
  await page.evaluate(
    (url) => window.__emit('deep-link://new-url', [url]),
    `tmgr://auth/callback?code=${CODE}&state=${state}`,
  );

  await expect.poll(() => accepts.length).toBe(1);
  await expect(page).not.toHaveURL(/\/login/);
});

test('desktop: a slow exchange of an older attempt does not replace a newer attempt', async ({
  page,
}) => {
  const { starts } = await loggedOutDesktop(page);
  let releaseAccept;
  const acceptGate = new Promise((resolve) => (releaseAccept = resolve));
  await page.route('**/api/auth/login/desktop/accept', async (route) => {
    await acceptGate;
    await route.fulfill({ json: ENVELOPE });
  });
  await page.goto('/login');
  await page.getByRole('button', { name: 'GitHub' }).click();
  await expect.poll(() => starts.length).toBe(1);
  const oldState = starts[0].searchParams.get('state');

  await page.evaluate(
    (url) => window.__emit('deep-link://new-url', [url]),
    `tmgr://auth/callback?code=${CODE}&state=${oldState}`,
  );
  await expect(page.getByText('Signing you in')).toBeVisible();
  await page.getByRole('button', { name: 'GitHub' }).click();
  await expect.poll(() => starts.length).toBe(2);
  releaseAccept();

  await page.waitForTimeout(500);
  await expect(page).toHaveURL(/\/login/);
  expect(await page.evaluate(() => localStorage.getItem('token'))).toBeNull();
});

test('desktop: Telegram is a button that opens the browser flow', async ({
  page,
}) => {
  await loggedOutDesktop(page);
  const telegramStarts = [];
  await page.route('**/api/auth/login/desktop/telegram?*', async (route) => {
    telegramStarts.push(route.request().url());
    await route.fulfill({ status: 204, body: '' });
  });
  await page.goto('/login');

  await page.getByRole('button', { name: 'Telegram' }).click();

  await expect.poll(() => telegramStarts.length).toBe(1);
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByText('Continue in your browser')).toBeHidden();
});

test('website relay: GitHub callback with a desktop state hands the code to the app, not to the site', async ({
  page,
}) => {
  await mockApp(page);
  const relayed = [];
  await page.route('**/api/auth/login/desktop/github/complete?*', async (route) => {
    relayed.push(new URL(route.request().url()));
    await route.fulfill({
      status: 302,
      headers: {
        location: `/desktop-auth/return#code=${CODE}&state=${'s'.repeat(43)}`,
      },
    });
  });
  const webLogins = [];
  await page.route('**/api/auth/login/github/redirect', async (route) => {
    webLogins.push(1);
    await route.fulfill({ json: ENVELOPE });
  });

  await page.goto(`/login/github?code=gh-code&state=desktop.${TX}`, {
    waitUntil: 'commit',
  });

  const open = page.getByRole('link', { name: 'Open TMGR' });
  await expect(open).toHaveAttribute(
    'href',
    `tmgr://auth/callback?code=${CODE}&state=${'s'.repeat(43)}`,
  );
  expect(relayed).toHaveLength(1);
  expect(relayed[0].searchParams.get('code')).toBe('gh-code');
  expect(relayed[0].searchParams.get('tx')).toBe(TX);
  expect(webLogins).toHaveLength(0);
  expect(new URL(page.url()).hash).toBe('');
  expect(
    await page.evaluate(() => JSON.parse(localStorage.getItem('token')).token),
  ).toBe('fixture');
});

test('website relay: a failed exchange sends an error link back to the app', async ({
  page,
}) => {
  await mockApp(page);
  await page.route('**/api/auth/login/desktop/google/complete?*', (route) =>
    route.fulfill({
      status: 302,
      headers: { location: '/desktop-auth/return#error=google' },
    }),
  );

  await page.goto(`/login/google?code=bad&state=desktop.${TX}`, {
    waitUntil: 'commit',
  });

  await expect(page.getByRole('link', { name: 'Open TMGR' })).toHaveAttribute(
    'href',
    'tmgr://auth/callback?error=google',
  );
  await expect(page.getByText('Sign-in did not finish')).toBeVisible();
});

test('website relay: a forged return fragment is not forwarded', async ({
  page,
}) => {
  await mockApp(page);

  await page.goto('/desktop-auth/return#code=x&state=javascript:alert(1)');

  await expect(page.getByText('This sign-in link is not valid')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open TMGR' })).toHaveCount(0);
});
