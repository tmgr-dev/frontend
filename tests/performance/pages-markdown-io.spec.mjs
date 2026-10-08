import { expect, test } from '@playwright/test';
import { strFromU8, strToU8, unzipSync, zipSync } from 'fflate';
import { mkdirSync, readFileSync } from 'node:fs';
import { mockApp } from './mockApp.mjs';

const shotDir = process.env.PAGES_SHOTS;
if (shotDir) mkdirSync(shotDir, { recursive: true });

const shot = async (page, name) => {
  if (!shotDir) return;
  for (const scheme of ['light', 'dark']) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.evaluate(
      (dark) => document.documentElement.classList.toggle('dark', dark),
      scheme === 'dark',
    );
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${shotDir}/${name}-${scheme}.png` });
  }
};

const PNG = Uint8Array.from(
  Buffer.from(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
    'base64',
  ),
);

const vaultZip = () =>
  zipSync({
    'Sample Vault/.obsidian/app.json': strToU8('{}'),
    'Sample Vault/Welcome.md': strToU8(
      '---\ntags: [intro]\n---\n# Welcome\n\nStart with [[Project Alpha]] and the [setup guide](docs/Setup%20Guide.md).\n\n![[diagram.png]]\n\n```\n[not a link](docs/Setup Guide.md)\n```\n',
    ),
    'Sample Vault/Project Alpha.md': strToU8('Overview. See [[Meeting notes|the meeting]].\n'),
    'Sample Vault/Project Alpha/Meeting notes.md': strToU8(
      '---\ntitle: Kickoff meeting\ntype: meeting\n---\nAgenda\n\n- [back](../Welcome.md)\n- [missing](../Nowhere.md)\n',
    ),
    'Sample Vault/docs/Setup Guide.md': strToU8('# Setup Guide\n\n![logo](img/logo.png)\n'),
    'Sample Vault/attachments/diagram.png': PNG,
    'Sample Vault/docs/img/logo.png': PNG,
    'Sample Vault/notes.pdf': strToU8('%PDF-1.4 sample'),
    '../escape.md': strToU8('# Escape'),
  });

const author = { kind: 'user', id: 1, name: 'Test User' };

const fakePages = async (page, seed) => {
  const state = { pages: seed.map((p) => ({ ...p })), files: [], nextId: 100, nextFile: 500 };
  const summary = (p) => ({
    id: p.id,
    title: p.title,
    slug: p.slug,
    type: p.type,
    parent_id: p.parent_id,
    position: p.id,
    pinned: false,
    updated_at: '2026-10-01T10:00:00Z',
  });
  const full = (p) => ({
    ...summary(p),
    workspace_id: 1,
    body: p.body,
    properties: p.properties ?? {},
    version: p.version,
    author,
    updated_by: author,
    created_at: '2026-10-01T10:00:00Z',
    backlinks: [],
    sections: [],
    files: state.files.filter((f) => f.page_id === p.id),
    following: false,
  });
  const find = (key) => state.pages.find((p) => String(p.id) === key || p.slug === key);

  await page.route('**/api/**/feature-toggles', (route) => {
    const toggle = (key) => ({ key, name: key, group: 'pages', type: 'boolean', enabled: true });
    return route.fulfill({
      json: {
        data: route.request().url().includes('/user/')
          ? {}
          : { board: toggle('board'), pages: toggle('pages') },
      },
    });
  });
  await page.route('**/api/files/**', (route) => {
    const path = new URL(route.request().url()).pathname.replace('/api/files/', '');
    if (path === 'presign-upload') {
      const body = route.request().postDataJSON();
      const key = `uploads/${state.nextFile}-${body.file_name}`;
      return route.fulfill({
        json: {
          data: {
            key,
            upload_url: `http://127.0.0.1:4179/storage-put/${encodeURIComponent(key)}`,
            method: 'PUT',
            content_type: body.content_type || 'application/octet-stream',
            max_bytes: 10_000_000,
          },
        },
      });
    }
    const content = /^(\d+)\/content$/.exec(path);
    if (content) return route.fulfill({ body: Buffer.from(PNG), contentType: 'image/png' });
    if (/^\d+\/signed-url$/.test(path)) return route.fulfill({ status: 503, json: {} });
    return route.fulfill({ json: { data: null } });
  });
  await page.route('**/storage-put/**', (route) => route.fulfill({ status: 200, body: '' }));
  await page.route('**/api/pages**', (route) => {
    const request = route.request();
    const method = request.method();
    const path = new URL(request.url()).pathname.replace('/api/pages', '').replace(/^\//, '');
    const json = (data, status = 200) => route.fulfill({ status, json: { data } });
    if (method === 'GET' && (path === 'tree' || path === '')) return json(state.pages.map(summary));
    if (method === 'POST' && path === '') {
      const body = request.postDataJSON();
      const id = state.nextId++;
      const created = {
        id,
        title: body.title,
        slug: `p-${id}`,
        type: body.type ?? 'plain',
        parent_id: body.parent_id ?? null,
        body: body.body ?? '',
        properties: body.properties ?? {},
        version: 1,
      };
      state.pages.push(created);
      return json(full(created), 201);
    }
    const filesMatch = /^(\d+)\/files$/.exec(path);
    if (filesMatch && method === 'POST') {
      const body = request.postDataJSON();
      const file = {
        id: state.nextFile++,
        page_id: Number(filesMatch[1]),
        name: body.file_name,
        mime_type: body.mime_type,
        size: body.size_bytes,
        created_at: '2026-10-01T10:00:00Z',
      };
      state.files.push(file);
      return json(file, 201);
    }
    const target = find(path);
    if (target && method === 'PATCH') {
      const body = request.postDataJSON();
      if (body.version !== target.version)
        return route.fulfill({ status: 409, json: { error: 'page_conflict', data: full(target) } });
      Object.assign(target, {
        body: body.body ?? target.body,
        properties: body.properties ?? target.properties,
        title: body.title ?? target.title,
        version: target.version + 1,
      });
      return json(full(target));
    }
    if (target && method === 'GET') return json(full(target));
    return json([]);
  });
  await page.route('**/api/workspaces/1/categories*', (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  return state;
};

const seed = [
  { id: 1, title: 'Handbook', slug: 'handbook', type: 'plain', parent_id: null, version: 1,
    body: 'See [Onboarding](tmgr://page/2) and ![chart](tmgr://file/7).\n\nOutside: [Elsewhere](tmgr://page/99)\n' },
  { id: 2, title: 'Onboarding', slug: 'onboarding', type: 'plain', parent_id: 1, version: 1,
    body: 'Back to [Handbook](tmgr://page/1).\n' },
  { id: 3, title: 'Sample Vault', slug: 'sample-vault', type: 'plain', parent_id: null, version: 1, body: '' },
];

test.describe('Pages Markdown import and export', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
    await mockApp(page);
  });

  test('export all downloads a zip with a folder tree, assets and relative links', async ({ page }) => {
    await fakePages(page, seed);
    await page.goto('/demo/pages');
    const download = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Export all' }).click();
    const file = await download;
    expect(file.suggestedFilename()).toBe('demo-pages.zip');
    const entries = unzipSync(new Uint8Array(readFileSync(await file.path())));
    const names = Object.keys(entries).sort();
    expect(names).toEqual(
      expect.arrayContaining(['Handbook.md', 'Handbook/Onboarding.md', 'Sample Vault.md']),
    );
    expect(names.some((n) => /^assets\/7-/.test(n))).toBe(true);
    const handbook = strFromU8(entries['Handbook.md']);
    expect(handbook).toMatch(/^---\n/);
    expect(handbook).toContain('title: Handbook');
    expect(handbook).toContain('](Handbook/Onboarding.md)');
    expect(handbook).toMatch(/!\[chart\]\(assets\/7-[^)]+\)/);
    expect(handbook).toContain('](tmgr://page/99)');
    expect(strFromU8(entries['Handbook/Onboarding.md'])).toContain('](../Handbook.md)');
  });

  test('single page export downloads a .md file', async ({ page }) => {
    await fakePages(page, seed);
    await page.goto('/demo/pages/onboarding');
    await page.getByTestId('page-more-menu').click();
    const download = page.waitForEvent('download');
    await page.getByRole('menuitem', { name: 'Export as Markdown' }).click();
    const file = await download;
    expect(file.suggestedFilename()).toBe('onboarding.md');
    const text = readFileSync(await file.path(), 'utf8');
    expect(text).toContain('title: Onboarding');
    expect(text).toContain('Back to [Handbook](tmgr://page/1).');
  });

  test('imports an Obsidian-style vault zip with preview, conflict policy and rewritten links', async ({ page }) => {
    const state = await fakePages(page, seed);
    await page.goto('/demo/pages');
    await page.getByRole('button', { name: 'Import', exact: true }).click();
    await page.getByTestId('pages-import-input').setInputFiles({
      name: 'sample-vault.zip',
      mimeType: 'application/zip',
      buffer: Buffer.from(vaultZip()),
    });
    const preview = page.getByTestId('pages-import-preview');
    await expect(preview).toBeVisible();
    await expect(page.getByTestId('pages-import-row')).toHaveCount(6);
    await expect(page.getByTestId('pages-import-conflict-badge')).toHaveCount(1);
    await expect(page.getByTestId('pages-import-policy')).toBeVisible();
    await page.getByTestId('pages-import-warnings').getByRole('button').click();
    await expect(page.getByTestId('pages-import-warnings')).toContainText('Nowhere.md');
    await expect(page.getByTestId('pages-import-warnings')).toContainText('escape.md');
    await shot(page, 'import-preview');

    await page.getByTestId('pages-import-submit').click();
    await expect(page.getByTestId('pages-import-summary')).toBeVisible();
    await expect(page.getByTestId('pages-import-created-link')).toHaveCount(6);
    await shot(page, 'import-summary');

    const created = state.pages.filter((p) => p.id >= 100);
    const byTitle = Object.fromEntries(created.map((p) => [p.title, p]));
    expect(Object.keys(byTitle).sort()).toEqual(
      ['Kickoff meeting', 'Project Alpha', 'Sample Vault (2)', 'Setup Guide', 'Welcome', 'docs'].sort(),
    );
    const root = byTitle['Sample Vault (2)'];
    expect(root.parent_id).toBeNull();
    expect(byTitle.Welcome.parent_id).toBe(root.id);
    expect(byTitle['Kickoff meeting'].parent_id).toBe(byTitle['Project Alpha'].id);
    expect(byTitle['Setup Guide'].parent_id).toBe(byTitle.docs.id);
    expect(byTitle['Kickoff meeting'].type).toBe('meeting');
    expect(byTitle.Welcome.body).toContain(`(tmgr://page/${byTitle['Project Alpha'].id})`);
    expect(byTitle.Welcome.body).toContain(`(tmgr://page/${byTitle['Setup Guide'].id})`);
    expect(byTitle.Welcome.body).toMatch(/!\[[^\]]*\]\(tmgr:\/\/file\/\d+\)/);
    expect(byTitle.Welcome.body).toContain('[not a link](docs/Setup Guide.md)');
    expect(byTitle.Welcome.body).not.toMatch(/^# Welcome/m);
    expect(byTitle['Setup Guide'].body).toMatch(/!\[logo\]\(tmgr:\/\/file\/\d+\)/);
    expect(byTitle['Kickoff meeting'].body).toContain(`(tmgr://page/${byTitle.Welcome.id})`);
    expect(state.files.map((f) => f.name).sort()).toEqual(['diagram.png', 'logo.png']);
  });
});
