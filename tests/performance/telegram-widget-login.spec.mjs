import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const TX = 't'.repeat(43);
const USER = { id: 42, first_name: 'Tele', auth_date: 1700000000, hash: 'aa' };
const ATTACKER = {
  id: 666,
  first_name: 'Mallory',
  auth_date: 1700000000,
  hash: 'ff',
};
const b64url = (value) =>
  Buffer.from(JSON.stringify(value)).toString('base64url');
const FAILED = 'Telegram sign-in failed, try again';

// The hash path of the live telegram-widget.js?22 (haveTgAuthResult + initWidget,
// .team/telegram-widget.live.js): it redeems #tgAuthResult synchronously while it
// evaluates. `__tgFire` stands in for the iframe/popup result, which arrives later.
const WIDGET_STUB = `(function () {
  var s = document.currentScript;
  var onauth = new Function('user', s.getAttribute('data-onauth'));
  window.__tgLoads = (window.__tgLoads || 0) + 1;
  window.__tgFire = function (user) {
    var el = document.querySelector('script[data-telegram-login]');
    new Function('user', el.getAttribute('data-onauth'))(user);
  };
  var re = /[#\\?\\&]tgAuthResult=([A-Za-z0-9\\-_=]*)$/, match, result = false;
  var locationHash = location.hash.toString();
  if (match = locationHash.match(re)) {
    location.hash = locationHash.replace(re, '');
    var data = (match[1] || '').replace(/-/g, '+').replace(/_/g, '/');
    var pad = data.length % 4;
    if (pad > 1) data += new Array(5 - pad).join('=');
    try { result = JSON.parse(window.atob(data)); } catch (e) {}
  }
  if (result) onauth(result);
  // Like the iframe's init auth_user event: a freshly signed payload per load.
  if (window.__tgInitAuth) {
    var n = window.__tgLoads;
    setTimeout(function () {
      window.__tgFire({ id: 42, auth_date: 1700000000, hash: 'init' + n });
    }, 0);
  }
})();`;

const recordWidgetAppends = (page) =>
  page.addInitScript(() => {
    window.__tgAppends = [];
    for (const method of ['appendChild', 'insertBefore']) {
      const original = Node.prototype[method];
      Node.prototype[method] = function (node, ...rest) {
        if (
          node?.tagName === 'SCRIPT' &&
          String(node.src).includes('telegram-widget.js')
        )
          window.__tgAppends.push(location.href);
        return original.call(this, node, ...rest);
      };
    }
  });

const stubWidget = (page, gate = null) =>
  page.route('https://telegram.org/js/telegram-widget.js*', async (route) => {
    await gate;
    await route.fulfill({ contentType: 'text/javascript', body: WIDGET_STUB });
  });

/** Logged-out visitor; the app's API is mocked so a successful sign-in can land. */
const loggedOut = async (page, response) => {
  await mockApp(page);
  await page.addInitScript(() => {
    if (!sessionStorage.getItem('fixture.loggedOut')) {
      sessionStorage.setItem('fixture.loggedOut', '1');
      localStorage.removeItem('token');
    }
  });
  await recordWidgetAppends(page);
  const posts = [];
  await page.route('**/api/auth/login/telegram/widget', async (route) => {
    posts.push({
      body: route.request().postDataJSON(),
      contentType: route.request().headers()['content-type'],
    });
    await route.fulfill(
      typeof response === 'function'
        ? response(route, posts.length - 1)
        : response,
    );
  });
  return posts;
};

const appends = (page) => page.evaluate(() => window.__tgAppends.length);
const loads = (page) => page.evaluate(() => window.__tgLoads ?? 0);
const fire = (page, ...users) =>
  page.evaluate(
    (list) => list.forEach((u) => setTimeout(() => window.__tgFire(u), 0)),
    users,
  );
const storage = (page) =>
  page.evaluate(() => ({
    local: { ...localStorage },
    session: { ...sessionStorage },
  }));

test('the widget callback posts the signed object once and signs in', async ({
  page,
}) => {
  const posts = await loggedOut(page, { json: { data: { token: 'tg-jwt' } } });
  await stubWidget(page);
  await page.goto('/login');
  await expect.poll(() => loads(page)).toBe(1);

  await fire(page, USER, USER);

  await expect(page).not.toHaveURL(/\/login/, { timeout: 10_000 });
  expect(posts).toHaveLength(1);
  expect(posts[0].body).toEqual(USER);
  expect(posts[0].contentType).toContain('application/json');
  expect(
    await page.evaluate(() => JSON.parse(localStorage.getItem('token')).token),
  ).toBe('tg-jwt');
});

for (const [status, response] of [
  [
    403,
    {
      status: 403,
      json: { message: 'Forbidden', error: 'telegram_login_rejected' },
    },
  ],
  [
    429,
    {
      status: 429,
      headers: { 'Retry-After': '30' },
      json: { message: 'Too many requests' },
    },
  ],
]) {
  test(`a ${status} shows the error, re-renders the widget and never re-posts the payload`, async ({
    page,
  }) => {
    const posts = await loggedOut(page, response);
    await stubWidget(page);
    await page.goto('/login');
    await expect.poll(() => loads(page)).toBe(1);
    const before = await storage(page);

    await fire(page, USER);

    await expect(page.getByText(FAILED)).toBeVisible();
    await expect.poll(() => appends(page)).toBe(2);
    await expect.poll(() => loads(page)).toBe(2);
    await fire(page, USER);
    await page.waitForTimeout(300);
    expect(posts).toHaveLength(1);
    await expect(page.getByText(FAILED)).toBeVisible();
    await expect(page).toHaveURL(/\/login$/);
    expect(await storage(page)).toEqual(before);
  });
}

for (const path of ['/login', '/register']) {
  test(`${path}: after two failures the widget waits for "Try again"`, async ({
    page,
  }) => {
    const posts = await loggedOut(page, {
      status: 403,
      json: { message: 'Forbidden', error: 'telegram_login_rejected' },
    });
    await page.addInitScript(() => (window.__tgInitAuth = true));
    await stubWidget(page);
    await page.goto(path);

    const retry = page.getByRole('button', { name: 'Try again' });
    await expect(retry).toBeVisible();
    await page.waitForTimeout(3000);
    expect(posts).toHaveLength(2);
    expect(await loads(page)).toBe(2);
    await expect(page.getByText(FAILED)).toBeVisible();

    await retry.click();
    await expect.poll(() => posts.length).toBe(3);
    await expect(retry).toBeVisible();
    await page.waitForTimeout(1000);
    expect(posts.map((p) => p.body.hash)).toEqual(['init1', 'init2', 'init3']);
    expect(await loads(page)).toBe(3);
    expect(await page.evaluate(() => localStorage.getItem('token'))).toBeNull();
  });
}

test('register: a rejected widget login shows the error status', async ({
  page,
}) => {
  const posts = await loggedOut(page, {
    status: 403,
    json: { message: 'Forbidden', error: 'telegram_login_rejected' },
  });
  await stubWidget(page);
  await page.goto('/register');
  await expect.poll(() => loads(page)).toBe(1);

  await fire(page, USER);

  await expect(page.getByText(FAILED)).toBeVisible();
  await expect.poll(() => appends(page)).toBe(2);
  expect(posts).toHaveLength(1);
  expect(await page.evaluate(() => localStorage.getItem('token'))).toBeNull();
});

const desktopCompletes = async (page) => {
  const completes = [];
  await page.route(
    '**/api/auth/login/desktop/telegram/complete',
    async (route) => {
      completes.push(1);
      await route.fulfill({ status: 204, body: '' });
    },
  );
  return completes;
};

for (const path of ['/login', '/register', `/desktop-auth/telegram?tx=${TX}`]) {
  for (const hash of [
    `#tgAuthResult=${b64url(ATTACKER)}`,
    `#a&tgAuthResult=${b64url(ATTACKER)}`,
    `#/?tgAuthResult=${b64url(ATTACKER)}`,
  ]) {
    const label = `${path.split('?')[0]}${hash.split('=')[0]}`;
    test(`a URL-borne result is dropped before the widget loads: ${label}`, async ({
      page,
    }) => {
      const posts = await loggedOut(page, {
        json: { data: { token: 'attacker-jwt' } },
      });
      const completes = await desktopCompletes(page);
      await stubWidget(page);

      await page.goto(path + hash);
      await expect.poll(() => loads(page)).toBeGreaterThan(0);
      await page.waitForTimeout(300);

      const hrefAtAppend = await page.evaluate(() => window.__tgAppends[0]);
      expect(hrefAtAppend).not.toContain('tgAuthResult');
      expect(page.url()).not.toContain('tgAuthResult');
      expect(posts).toHaveLength(0);
      expect(completes).toHaveLength(0);
      expect(
        await page.evaluate(() => localStorage.getItem('token')),
      ).toBeNull();
    });
  }
}

test('a tgAuthResult in the query string is not redeemed', async ({ page }) => {
  const posts = await loggedOut(page, {
    json: { data: { token: 'attacker-jwt' } },
  });
  await stubWidget(page);

  await page.goto(`/login?tgAuthResult=${b64url(ATTACKER)}`);
  await expect.poll(() => loads(page)).toBe(1);
  await page.waitForTimeout(300);

  expect(posts).toHaveLength(0);
  expect(await page.evaluate(() => localStorage.getItem('token'))).toBeNull();
});

for (const [how, putHash] of [
  [
    'without a remount',
    (hash) => history.replaceState(history.state, '', `#${hash}`),
  ],
  ['by a fragment navigation', (hash) => (location.hash = hash)],
]) {
  test(`a result re-added to the URL ${how} while the widget loads is not redeemed`, async ({
    page,
  }) => {
    const posts = await loggedOut(page, {
      json: { data: { token: 'tg-jwt' } },
    });
    let release;
    await stubWidget(page, new Promise((resolve) => (release = resolve)));
    await page.goto('/login');
    await expect.poll(() => appends(page)).toBe(1);

    await page.evaluate(putHash, `tgAuthResult=${b64url(ATTACKER)}`);
    await page.waitForTimeout(200);
    release();
    await expect.poll(() => loads(page)).toBeGreaterThan(0);
    await page.waitForTimeout(300);
    expect(posts).toHaveLength(0);

    await fire(page, USER);
    await expect(page).not.toHaveURL(/\/login/, { timeout: 10_000 });
    expect(posts.map((p) => p.body)).toEqual([USER]);
  });
}

test('register: the widget callback signs in and lands in the app', async ({
  page,
}) => {
  const posts = await loggedOut(page, { json: { data: { token: 'tg-jwt' } } });
  await stubWidget(page);
  await page.goto('/register');
  await expect.poll(() => loads(page)).toBe(1);

  await fire(page, USER);

  await expect(page).not.toHaveURL(/\/register/, { timeout: 10_000 });
  expect(posts.map((p) => p.body)).toEqual([USER]);
  expect(
    await page.evaluate(() => JSON.parse(localStorage.getItem('token')).token),
  ).toBe('tg-jwt');
});

const INVITATION = [
  'workspace invitation',
  "localStorage.setItem('workspace.invitation', 'inv-tok')",
  /\/workspaces\/invitations\/inv-tok$/,
];
for (const [path, [what, seed, landing]] of [
  [
    '/login',
    [
      'return path',
      "sessionStorage.setItem('auth.return.to', '/stats')",
      /\/stats$/,
    ],
  ],
  ['/login', INVITATION],
  ['/register', INVITATION],
]) {
  test(`${path}: a Telegram sign-in honours the ${what}`, async ({ page }) => {
    await loggedOut(page, { json: { data: { token: 'tg-jwt' } } });
    await page.addInitScript(
      `if (!sessionStorage.getItem('fixture.seeded')) {
        sessionStorage.setItem('fixture.seeded', '1');
        ${seed};
      }`,
    );
    await stubWidget(page);
    await page.goto(path);
    await expect.poll(() => loads(page)).toBe(1);

    await fire(page, USER);

    await expect(page).toHaveURL(landing, { timeout: 10_000 });
  });
}

test('a result followed by more hash params is dropped and the widget still loads once', async ({
  page,
}) => {
  const posts = await loggedOut(page, {
    json: { data: { token: 'attacker-jwt' } },
  });
  await stubWidget(page);

  await page.goto(`/login#tgAuthResult=${b64url(ATTACKER)}&y=1`);
  await expect.poll(() => loads(page)).toBe(1);
  await page.waitForTimeout(300);

  const hrefs = await page.evaluate(() => window.__tgAppends);
  expect(hrefs).toHaveLength(1);
  expect(hrefs[0]).not.toContain('tgAuthResult');
  expect(page.url()).not.toContain('tgAuthResult');
  expect(posts).toHaveLength(0);
});

test('after a rejection a fresh widget login signs in', async ({ page }) => {
  const responses = [
    { status: 403, json: { error: 'telegram_login_rejected' } },
    { json: { data: { token: 'tg-jwt' } } },
  ];
  const posts = await loggedOut(page, (route, n) => responses[n]);
  await stubWidget(page);
  await page.goto('/login');
  await expect.poll(() => loads(page)).toBe(1);

  await fire(page, USER);
  await expect(page.getByText(FAILED)).toBeVisible();
  await expect.poll(() => loads(page)).toBe(2);
  const fresh = { ...USER, auth_date: 1700000100, hash: 'bb' };
  await fire(page, fresh);

  await expect(page).not.toHaveURL(/\/login/, { timeout: 10_000 });
  expect(posts.map((p) => p.body)).toEqual([USER, fresh]);
});

for (const path of ['/login', '/register']) {
  test(`desktop app: ${path} never loads the Telegram widget`, async ({
    page,
  }) => {
    await loggedOut(page, { json: { data: { token: 'tg-jwt' } } });
    await page.addInitScript(() => {
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' } },
        invoke: () => Promise.resolve(null),
        transformCallback: () => 0,
      };
    });
    await stubWidget(page);
    await page.goto(path);
    await expect(page.getByRole('button', { name: 'Telegram' })).toBeVisible();
    await page.waitForTimeout(300);

    expect(await appends(page)).toBe(0);
    expect(await loads(page)).toBe(0);
  });

  test(`${path} with the widget logs no console errors`, async ({ page }) => {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (msg) => {
      if (msg.type() === 'error') errors.push(msg.text());
    });
    await loggedOut(page, { json: { data: { token: 'tg-jwt' } } });
    await stubWidget(page);
    await page.goto(path);
    await expect.poll(() => loads(page)).toBe(1);
    await page.waitForTimeout(300);

    expect(errors).toEqual([]);
  });
}
